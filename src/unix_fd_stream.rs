use std::{
    os::{
        fd::{AsRawFd, RawFd},
        unix::net::UnixStream,
    },
    path::Path,
    ptr,
};

use crate::wl::debug_println;

const FD_BUFFER_LEN: usize = 32;

#[repr(C)]
struct iovec {
    iov_base: *mut u8,
    iov_len: usize,
}

#[repr(C)]
struct msghdr {
    msg_name: *mut std::ffi::c_void,
    msg_namelen: u32,
    msg_iov: *mut iovec,
    msg_iovlen: usize,
    msg_control: *mut std::ffi::c_void,
    msg_controllen: usize,
    msg_flags: i32,
}

#[repr(C)]
struct cmsghdr {
    cmsg_len: usize,
    cmsg_level: i32,
    cmsg_type: i32,
    // followed by u8[] data
}

#[cfg(not(target_arch = "mips"))]
const SOL_SOCKET: i32 = 1;

#[cfg(target_arch = "mips")]
const SOL_SOCKET: i32 = 0xffff;

const SCM_RIGHTS: i32 = 0x01;
const MSG_CMSG_CLOEXEC: i32 = 0x40000000;

const CMSG_FD_OFFSET: usize = cmsg_align(std::mem::size_of::<cmsghdr>());

const CTRL_BUFFER_SIZE: usize = cmsg_space(FD_BUFFER_LEN * std::mem::size_of::<RawFd>());

#[repr(C, align(8))]
struct AlignedCmsghdr([u8; CTRL_BUFFER_SIZE]);

impl cmsghdr {
    fn fds_into(&self, fd_buffer: &mut WLFdBuffer) -> std::io::Result<()> {
        if self.cmsg_level == SOL_SOCKET && self.cmsg_type == SCM_RIGHTS {
            let data_ptr = unsafe { (self as *const cmsghdr).add(1) as *const RawFd };
            let fd_count = (self.cmsg_len - CMSG_FD_OFFSET) / std::mem::size_of::<RawFd>();
            let data_slice = unsafe { std::slice::from_raw_parts(data_ptr, fd_count) };
            fd_buffer.push_in_fds(data_slice)?;

            debug_println!("Received {} file descriptors", fd_count);
        }
        Ok(())
    }
}

const fn cmsg_align(len: usize) -> usize {
    let align_to = std::mem::size_of::<usize>();
    (len + align_to - 1) & !(align_to - 1)
}

const fn cmsg_space(len: usize) -> usize {
    cmsg_align(std::mem::size_of::<cmsghdr>()) + cmsg_align(len)
}

unsafe extern "C" {
    fn recvmsg(sockfd: RawFd, msg: *mut msghdr, flags: i32) -> isize;
    fn sendmsg(sockfd: RawFd, msg: *const msghdr, flags: i32) -> isize;
    fn pipe2(fd: *mut RawFd, flags: i32) -> RawFd;
    fn close(fd: RawFd) -> i32;
    fn write(fd: RawFd, buf: *const u8, count: usize) -> isize;
}

pub struct WLFdBuffer {
    in_fds: [RawFd; FD_BUFFER_LEN],
    in_fd_count: usize,
    in_fds_cursor: usize,
    out_fds: [RawFd; FD_BUFFER_LEN],
}

impl WLFdBuffer {
    pub fn new() -> Self {
        Self {
            in_fds: [0; FD_BUFFER_LEN],
            in_fd_count: 0,
            in_fds_cursor: 0,
            out_fds: [0; FD_BUFFER_LEN],
        }
    }

    pub fn pop_last_in_fd(&mut self) -> Option<RawFd> {
        // this is a ring buffer, so we need to wrap around if we reach the end of the buffer
        if self.in_fd_count == 0 {
            return None;
        }
        let fd = self.in_fds[self.in_fds_cursor];
        self.in_fds_cursor = (self.in_fds_cursor + 1) % self.in_fds.len();
        self.in_fd_count -= 1;
        Some(fd)
    }

    fn push_in_fds(&mut self, fds: &[RawFd]) -> std::io::Result<()> {
        // this is a ring buffer, so we need to wrap around if we reach the end of the buffer
        if fds.len() > self.in_fds.len() - self.in_fd_count {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Not enough space in buffer for file descriptors",
            ));
        }
        for &fd in fds {
            self.in_fds[(self.in_fds_cursor + self.in_fd_count) % self.in_fds.len()] = fd;
            self.in_fd_count += 1;
        }
        Ok(())
    }

    pub fn fd_write_and_close(&self, fd: RawFd, data: &[u8]) -> std::io::Result<()> {
        let mut bytes_written = 0;
        let mut zero_retry_count = 0;
        while bytes_written < data.len() {
            let bytes_written_or_err = unsafe {
                crate::unix_fd_stream::write(
                    fd,
                    data.as_ptr().add(bytes_written),
                    data.len() - bytes_written,
                )
            };
            if bytes_written_or_err > 0 {
                bytes_written += bytes_written_or_err as usize;
            } else if bytes_written_or_err == 0 {
                zero_retry_count += 1;
                if zero_retry_count > 10 {
                    unsafe { close(fd) };
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::WriteZero,
                        "write returned zero too many times",
                    ));
                }
                continue; // retry on zero write, which can happen with some special files
            } else {
                let error = std::io::Error::last_os_error();
                match error.raw_os_error() {
                    Some(4) => continue, // EINTR
                    Some(32) => break,   // EPIPE, the reader has closed the pipe
                    _ => {
                        unsafe { close(fd) };
                        return Err(error);
                    }
                }
            }
        }
        unsafe { close(fd) };
        Ok(())
    }
}

pub struct UnixFdStream {
    stream: UnixStream,
    stream_fd: RawFd,
}

impl UnixFdStream {
    pub fn connect(path: &Path) -> std::io::Result<Self> {
        let stream = UnixStream::connect(path)?;
        let stream_fd = stream.as_raw_fd();
        Ok(Self { stream, stream_fd })
    }

    pub fn read(
        &mut self,
        buffer: &mut [u8],
        fd_buffer: &mut WLFdBuffer,
    ) -> std::io::Result<usize> {
        let mut iovec = iovec {
            iov_base: buffer.as_mut_ptr(),
            iov_len: buffer.len(),
        };

        loop {
            let mut ctrl_buffer = AlignedCmsghdr([0u8; CTRL_BUFFER_SIZE]);

            let mut msg = msghdr {
                msg_name: ptr::null_mut(),
                msg_namelen: 0,
                msg_iov: &mut iovec,
                msg_iovlen: 1,
                msg_control: ctrl_buffer.0.as_mut_ptr() as *mut std::ffi::c_void,
                msg_controllen: ctrl_buffer.0.len(),
                msg_flags: 0,
            };

            let bytes_read_or_err = unsafe { recvmsg(self.stream_fd, &mut msg, MSG_CMSG_CLOEXEC) };

            if bytes_read_or_err < 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::Interrupted {
                    continue; // retry on EINTR
                } else {
                    return Err(error);
                }
            }
            if bytes_read_or_err == 0 {
                return Ok(0); // EOF
            }

            let mut ctrl_buf_cursor = 0;

            while ctrl_buf_cursor < msg.msg_controllen {
                let cmsg = unsafe { &mut *(msg.msg_control.add(ctrl_buf_cursor) as *mut cmsghdr) };
                if cmsg.cmsg_len == 0 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Invalid control message with zero length",
                    ));
                }
                cmsg.fds_into(fd_buffer)?;
                ctrl_buf_cursor += cmsg_align(cmsg.cmsg_len);
            }

            return Ok(bytes_read_or_err as usize);
        }
    }

    #[allow(unused_assignments)]
    pub fn write(&mut self, buff: &[u8]) -> std::io::Result<isize> {
        let mut iov = iovec {
            iov_base: buff.as_ptr() as *mut _,
            iov_len: buff.len(),
        };
        let msg = msghdr {
            msg_name: ptr::null_mut(),
            msg_namelen: 0,
            msg_iov: &mut iov,
            msg_iovlen: 1,
            msg_control: ptr::null_mut(),
            msg_controllen: 0,
            msg_flags: 0,
        };

        let mut total_bytes_sent = 0;
        loop {
            let bytes_sent_or_err = unsafe { sendmsg(self.stream_fd, &msg, 0) };

            if bytes_sent_or_err < 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::Interrupted {
                    continue; // retry on EINTR
                } else {
                    return Err(error);
                }
            }

            // we need to adjust the iovec to point to the remaining data that needs to be sent
            total_bytes_sent += bytes_sent_or_err as usize;

            if total_bytes_sent < buff.len() {
                iov.iov_base = unsafe { buff.as_ptr().add(total_bytes_sent) as *mut _ };
                iov.iov_len = buff.len() - total_bytes_sent;

                continue; // retry sending the remaining data
            }

            return Ok(total_bytes_sent as isize);
        }
    }
}
