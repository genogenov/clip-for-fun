use std::{os::{fd::{AsRawFd, RawFd}, unix::net::UnixStream}, ptr};

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
    fn fds_into(&self, buf: &mut [RawFd], buf_idx: usize) -> std::io::Result<usize> {
        if self.cmsg_level == SOL_SOCKET && self.cmsg_type == SCM_RIGHTS {
            let data_ptr = unsafe { (self as *const cmsghdr).add(1) as *const RawFd };
            let data_slice =
                unsafe { std::slice::from_raw_parts(data_ptr, (self.cmsg_len - CMSG_FD_OFFSET) / std::mem::size_of::<RawFd>()) };
            let fd_count = data_slice.len();
            if buf.len() - buf_idx < fd_count {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Not enough space in buffer for file descriptors",
                ));
            }
            buf[buf_idx..buf_idx + fd_count].copy_from_slice(&data_slice[..]);
            return Ok(fd_count);
        }
        Ok(0)
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
}

pub struct UnixFdStream {
    stream: UnixStream,
    stream_fd: RawFd,
    in_fds: [RawFd; FD_BUFFER_LEN],
    in_fd_count: usize,
    out_fds: [RawFd; FD_BUFFER_LEN],
}

impl UnixFdStream {
    pub fn connect(path: &str) -> std::io::Result<Self> {
        let stream = UnixStream::connect(path)?;
        let stream_fd = stream.as_raw_fd();
        Ok(Self {
            stream,
            stream_fd,
            in_fds: [0; FD_BUFFER_LEN],
            in_fd_count: 0,
            out_fds: [0; FD_BUFFER_LEN],
        })
    }

    pub fn pop_last_in_fd(&mut self) -> Option<RawFd> {
        if self.in_fd_count == 0 {
            None
        } else {
            self.in_fd_count -= 1;
            Some(self.in_fds[self.in_fd_count])
        }
    }

    pub fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let mut iovec = iovec {
            iov_base: buffer.as_mut_ptr(),
            iov_len: buffer.len(),
        };

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
                return self.read(buffer); // retry on EINTR
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
            let fd_count = cmsg.fds_into(&mut self.in_fds, self.in_fd_count)?;
            self.in_fd_count += fd_count;
            ctrl_buf_cursor += cmsg_align(cmsg.cmsg_len);
        }

        return Ok(bytes_read_or_err as usize);
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
                // println!("Partial write");
                iov.iov_base = unsafe { buff.as_ptr().add(total_bytes_sent) as *mut _ };
                iov.iov_len = buff.len() - total_bytes_sent;

                continue; // retry sending the remaining data
            }

            return Ok(bytes_sent_or_err);
        }
    }
}