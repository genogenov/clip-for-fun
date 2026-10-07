use std::{
    fs::File,
    io::{ErrorKind, Write},
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
        unix::net::UnixStream,
    },
    path::Path,
    ptr,
};

const FD_BUFFER_LEN: usize = 32;
const USIZE: usize = size_of::<usize>();

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

#[cfg(not(any(
    target_arch = "mips",
    target_arch = "mips32r6",
    target_arch = "mips64",
    target_arch = "mips64r6",
    target_arch = "sparc",
    target_arch = "sparc64"
)))]

const SOL_SOCKET: i32 = 1;

#[cfg(any(
    target_arch = "mips",
    target_arch = "mips32r6",
    target_arch = "mips64",
    target_arch = "mips64r6",
    target_arch = "sparc",
    target_arch = "sparc64"
))]
const SOL_SOCKET: i32 = 0xffff;

const SCM_RIGHTS: i32 = 0x01;

const MSG_CTRUNC: i32 = 0x8;
const MSG_CMSG_CLOEXEC: i32 = 0x40000000;

const CMSG_FD_OFFSET: usize = cmsg_align(std::mem::size_of::<cmsghdr>());

const CTRL_BUFFER_SIZE: usize = cmsg_space(FD_BUFFER_LEN * std::mem::size_of::<RawFd>());

#[repr(C, align(8))]
struct AlignedCmsghdr([u8; CTRL_BUFFER_SIZE]);

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
    // fn pipe2(fd: *mut RawFd, flags: i32) -> RawFd;
    // fn close(fd: RawFd) -> i32;
    // fn write(fd: RawFd, buf: *const u8, count: usize) -> isize;
}

pub struct InFdBuffer {
    in_fds: [Option<OwnedFd>; FD_BUFFER_LEN],
    in_fd_count: usize,
    in_fds_cursor: usize,
}

pub struct OutFdBuffer {
    out_fds: [Option<OwnedFd>; FD_BUFFER_LEN],
    out_fd_count: usize,
}

pub trait FdWriteAndClose {
    fn fd_write_and_close(self, buf: &[u8]) -> std::io::Result<()>;
}

impl FdWriteAndClose for OwnedFd {
    fn fd_write_and_close(self, buf: &[u8]) -> std::io::Result<()> {
        match File::from(self).write_all(buf) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == ErrorKind::BrokenPipe => Ok(()),
            Err(e) => Err(e),
        }
    }
}
impl InFdBuffer {
    pub fn new() -> Self {
        Self {
            in_fds: [const { None }; FD_BUFFER_LEN],
            in_fd_count: 0,
            in_fds_cursor: 0,
        }
    }

    pub fn pop_last_in_fd(&mut self) -> Option<OwnedFd> {
        // this is a ring buffer, so we need to wrap around if we reach the end of the buffer
        if self.in_fd_count == 0 {
            return None;
        }
        let fd = self.in_fds[self.in_fds_cursor].take();
        self.in_fds_cursor = (self.in_fds_cursor + 1) % self.in_fds.len();
        self.in_fd_count -= 1;
        fd
    }

    fn push_in_fd(&mut self, fd: OwnedFd) -> std::io::Result<()> {
        // this is a ring buffer, so we need to wrap around if we reach the end of the buffer
        if self.in_fd_count >= self.in_fds.len() {
            return Err(std::io::Error::other(
                "Not enough space in buffer for file descriptors",
            ));
        }
        self.in_fds[(self.in_fds_cursor + self.in_fd_count) % self.in_fds.len()] = Some(fd);
        self.in_fd_count += 1;
        Ok(())
    }
}

impl OutFdBuffer {
    pub fn new() -> Self {
        Self {
            out_fds: [const { None }; FD_BUFFER_LEN],
            out_fd_count: 0,
        }
    }

    pub fn push_out_fd(&mut self, fd: OwnedFd) -> std::io::Result<()> {
        if self.out_fd_count >= self.out_fds.len() {
            return Err(std::io::Error::other(
                "Not enough space in output buffer for file descriptors",
            ));
        }
        self.out_fds[self.out_fd_count] = Some(fd);
        self.out_fd_count += 1;
        Ok(())
    }

    pub fn peek_out_fds(&self) -> &[Option<OwnedFd>] {
        &self.out_fds[..self.out_fd_count]
    }

    pub fn truncate_out(&mut self, truncate_count: usize) {
        if truncate_count > self.out_fd_count {
            return;
        }
        for slot in &mut self.out_fds[self.out_fd_count - truncate_count..self.out_fd_count] {
            *slot = None;
        }
        self.out_fd_count -= truncate_count;
    }

    pub fn clear_and_close_out_fds(&mut self) {
        for slot in &mut self.out_fds[..self.out_fd_count] {
            *slot = None;
        }
        self.out_fd_count = 0;
    }
}

pub struct UnixFdStream {
    stream_fd: OwnedFd,
}

impl From<UnixStream> for UnixFdStream {
    fn from(stream: UnixStream) -> Self {
        Self {
            stream_fd: stream.into(),
        }
    }
}

impl UnixFdStream {
    pub fn connect(path: &Path) -> std::io::Result<Self> {
        Ok(UnixStream::connect(path)?.into())
    }

    pub fn read(&self, buffer: &mut [u8], fd_buffer: &mut InFdBuffer) -> std::io::Result<usize> {
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

            // SAFETY: msg points at iov/control buffers for the call; the kernel writes at most their lengths.
            let bytes_read_or_err =
                unsafe { recvmsg(self.stream_fd.as_raw_fd(), &mut msg, MSG_CMSG_CLOEXEC) };

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

            if msg.msg_flags & MSG_CTRUNC != 0 {
                return Err(std::io::Error::other("Control message truncated"));
            }
            let mut ctrl = &ctrl_buffer.0[..msg.msg_controllen];
            while let Some(hdr) = ctrl.first_chunk::<CMSG_FD_OFFSET>() {
                let len = usize::from_ne_bytes(hdr[..USIZE].try_into().unwrap());
                let level = i32::from_ne_bytes(hdr[USIZE..USIZE + 4].try_into().unwrap());
                let kind = i32::from_ne_bytes(hdr[USIZE + 4..USIZE + 8].try_into().unwrap());
                let data = ctrl
                    .get(CMSG_FD_OFFSET..len)
                    .ok_or_else(|| std::io::Error::other("Malformed control message"))?;
                if level == SOL_SOCKET && kind == SCM_RIGHTS {
                    // keep wrapping all incoming FDs in OwnedFd to prevent leaks
                    // this is arguably an overkill given this will terminate the process, but adding it here for extra safety..
                    let mut pushed = Ok(());
                    for raw in data.as_chunks::<4>().0 {
                        // SAFETY: the kernel just installed this fd via SCM_RIGHTS; nothing else owns it.
                        let fd = unsafe { OwnedFd::from_raw_fd(i32::from_ne_bytes(*raw)) };
                        if pushed.is_ok() {
                            pushed = fd_buffer.push_in_fd(fd);
                        }
                    }

                    pushed?;
                }
                ctrl = ctrl.get(cmsg_align(len)..).unwrap_or(&[]);
            }

            return Ok(bytes_read_or_err as usize);
        }
    }

    pub fn write(&self, data: &[u8], fds: &[Option<OwnedFd>]) -> std::io::Result<()> {
        if fds.is_empty() {
            self.send_all(&mut [], data)
        } else if fds.len() > FD_BUFFER_LEN {
            Err(std::io::Error::other("Too many file descriptors to send"))
        } else {
            let fds_bytes_len: usize = fds.len() * std::mem::size_of::<RawFd>();
            let cmsg_space = cmsg_space(fds_bytes_len);
            let mut ctrl_buffer: AlignedCmsghdr = AlignedCmsghdr([0u8; CTRL_BUFFER_SIZE]);
            let mut_ctrl_buffer = &mut ctrl_buffer.0;
            mut_ctrl_buffer[..USIZE]
                .copy_from_slice(&(CMSG_FD_OFFSET + fds_bytes_len).to_ne_bytes());
            mut_ctrl_buffer[USIZE..USIZE + 4].copy_from_slice(&SOL_SOCKET.to_ne_bytes());
            mut_ctrl_buffer[USIZE + 4..USIZE + 8].copy_from_slice(&SCM_RIGHTS.to_ne_bytes());
            for (dst, fd) in mut_ctrl_buffer[CMSG_FD_OFFSET..]
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(fds.iter().flatten().map(AsRawFd::as_raw_fd))
            {
                dst.copy_from_slice(&fd.to_ne_bytes());
            }

            self.send_all(&mut mut_ctrl_buffer[..cmsg_space], data)
        }
    }

    fn send_all(&self, msg_control: &mut [u8], buff: &[u8]) -> std::io::Result<()> {
        let mut total_bytes_sent = 0;
        let (mut control, mut ctrl_length) = match msg_control {
            [] => (ptr::null_mut(), 0),
            _ => (
                msg_control.as_mut_ptr() as *mut std::ffi::c_void,
                msg_control.len(),
            ),
        };
        loop {
            let mut iov = iovec {
                iov_base: buff[total_bytes_sent..].as_ptr() as *mut _,
                iov_len: buff.len() - total_bytes_sent,
            };

            let msg = msghdr {
                msg_name: ptr::null_mut(),
                msg_namelen: 0,
                msg_iov: &mut iov,
                msg_iovlen: 1,
                msg_control: control,
                msg_controllen: ctrl_length,
                msg_flags: 0,
            };

            // SAFETY: msg, iov and control point at buffers for the call; the kernel only reads them.
            let bytes_sent_or_err = unsafe { sendmsg(self.stream_fd.as_raw_fd(), &msg, 0) };

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
                // we also need to detach the ctrl buffer to avoid resending the FDs - those went with the first partial send.
                control = ptr::null_mut();
                ctrl_length = 0;
                continue; // retry sending the remaining data
            }

            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Read, time::Duration};

    fn dev_null() -> OwnedFd {
        File::open("/dev/null").unwrap().into()
    }

    // Reads EOF only once every other handle to the peer socket is closed.
    fn assert_peer_closed(mut keep: UnixStream) {
        keep.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
        assert_eq!(keep.read(&mut [0u8; 1]).unwrap(), 0);
    }

    #[test]
    fn in_fds_pop_in_fifo_order_across_wraparound() {
        let mut buf = InFdBuffer::new();
        // 3 does not divide FD_BUFFER_LEN, so some rounds straddle the wrap point.
        for _ in 0..FD_BUFFER_LEN {
            let fds: [OwnedFd; 3] = std::array::from_fn(|_| dev_null());
            let expected = fds.each_ref().map(|fd| fd.as_raw_fd());
            for fd in fds {
                buf.push_in_fd(fd).unwrap();
            }
            for raw in expected {
                assert_eq!(buf.pop_last_in_fd().unwrap().as_raw_fd(), raw);
            }
        }
        assert!(buf.pop_last_in_fd().is_none());
    }

    #[test]
    fn in_fds_overflow_is_an_error() {
        let mut buf = InFdBuffer::new();
        for _ in 0..FD_BUFFER_LEN {
            buf.push_in_fd(dev_null()).unwrap();
        }
        assert!(buf.push_in_fd(dev_null()).is_err());
    }

    #[test]
    fn queued_fds_are_closed_when_the_buffer_is_dropped() {
        let (keep_in, queued_in) = UnixStream::pair().unwrap();
        let (keep_out, queued_out) = UnixStream::pair().unwrap();
        let mut in_fds = InFdBuffer::new();
        let mut out_fds = OutFdBuffer::new();
        in_fds.push_in_fd(queued_in.into()).unwrap();
        out_fds.push_out_fd(queued_out.into()).unwrap();
        drop(in_fds);
        drop(out_fds);
        assert_peer_closed(keep_in);
        assert_peer_closed(keep_out);
    }

    #[test]
    fn write_rejects_more_fds_than_the_control_buffer_holds() {
        let (a, _b) = UnixStream::pair().unwrap();
        let stream = UnixFdStream::from(a);
        let fds: Vec<Option<OwnedFd>> = (0..=FD_BUFFER_LEN).map(|_| Some(dev_null())).collect();
        assert!(stream.write(b"x", &fds).is_err());
    }
}
