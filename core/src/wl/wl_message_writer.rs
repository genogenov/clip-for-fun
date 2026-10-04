use std::{mem::forget, os::fd::OwnedFd};

use crate::{
    unix_fd_stream::FdBuffer,
    wl::{
        objects::{MessageHeader, WlObject, WlStr},
        wl_message_router::WlPendingId,
    },
};

pub struct WlMessageWriter<'a> {
    buf: &'a mut [u8],
    write_position: &'a mut usize,
    start_position: usize,
    fds: &'a mut FdBuffer,
    fd_count: usize,
}

#[repr(transparent)]
pub struct WlMessageWriterGuard<'a>(WlMessageWriter<'a>);

impl<'a> WlMessageWriterGuard<'a> {
    #[inline(always)]
    pub fn pack_new_object_id(&mut self, local_id: &WlPendingId) -> std::io::Result<()> {
        self.pack_u32(local_id.id())?;
        Ok(())
    }

    #[inline(always)]
    pub fn pack_u32(&mut self, value: u32) -> std::io::Result<()> {
        self.0
            .buf
            .get_mut(*self.0.write_position..*self.0.write_position + 4)
            .ok_or_else(|| std::io::Error::other("message does not fit in the write buffer"))?
            .copy_from_slice(&value.to_ne_bytes());
        *self.0.write_position += 4;
        Ok(())
    }

    pub fn pack_fd(&mut self, fd: OwnedFd) -> std::io::Result<()> {
        self.0.fds.push_out_fd(fd)?;
        self.0.fd_count += 1;
        Ok(())
    }

    #[inline(always)]
    pub fn pack_wl_str(&mut self, s: &WlStr) -> std::io::Result<()> {
        // the bytes in wl_str are already prefixed with the length, and there is null terminator at the end, so we can just copy them directly to the write buffer
        let len = s.wl_bytes.len() as u32;
        self.0
            .buf
            .get_mut(*self.0.write_position..*self.0.write_position + len as usize)
            .ok_or_else(|| std::io::Error::other("message does not fit in the write buffer"))?
            .copy_from_slice(s.wl_bytes);
        *self.0.write_position += len as usize;
        // we also need to ensure the string is 4 byte aligned by adding padding if necessary
        let padding = (4 - (s.wl_bytes.len() % 4)) % 4;
        self.0
            .buf
            .get_mut(*self.0.write_position..*self.0.write_position + padding)
            .ok_or_else(|| std::io::Error::other("message does not fit in the write buffer"))?
            .fill(0);
        *self.0.write_position += padding;
        Ok(())
    }

    #[inline(always)]
    pub fn pack_str(&mut self, s: &str) -> std::io::Result<()> {
        // we need to pack the string as len + bytes + null terminator and ensure it is 4 byte aligned. The len is the str bytes + the null terminator.

        let needed = 4 + (s.len() + 1).next_multiple_of(4);
        self.0
            .write_position
            .checked_add(needed)
            .filter(|&end_cursor| end_cursor <= self.0.buf.len())
            .ok_or_else(|| std::io::Error::other("message does not fit in the write buffer"))?;
        let len = s.len() as u32 + 1;
        self.pack_u32(len)?;
        self.0.buf[*self.0.write_position..*self.0.write_position + s.len()]
            .copy_from_slice(s.as_bytes());
        *self.0.write_position += s.len();
        self.0.buf[*self.0.write_position] = 0; // null terminator
        *self.0.write_position += 1; // move past null terminator
        // we also need to ensure the string is 4 byte aligned by adding padding if necessary
        let padding = ((4 - (len % 4)) % 4) as usize;
        self.0.buf[*self.0.write_position..*self.0.write_position + padding].fill(0);
        *self.0.write_position += padding;
        Ok(())
    }

    #[inline(always)]
    pub fn end(self) {
        let message_length = (*self.0.write_position - self.0.start_position) as u32;
        let word: &mut [u8; 4] = (&mut self.0.buf
            [self.0.start_position + 4..self.0.start_position + 8])
            .try_into()
            .unwrap();
        *word = (u32::from_ne_bytes(*word) | message_length << 16).to_ne_bytes();

        // skip the destructor - this is our "commit".
        forget(self);
    }

    #[inline(always)]
    fn begin<T: WlObject>(&mut self, op: T::Ops, type_id: u32) -> std::io::Result<()> {
        let opcode: u16 = op.into();

        let buf = &mut self
            .0
            .buf
            .get_mut(*self.0.write_position..*self.0.write_position + 8)
            .ok_or_else(|| std::io::Error::other("message does not fit in the write buffer"))?;
        buf[0..4].copy_from_slice(&type_id.to_ne_bytes());
        buf[4..8].copy_from_slice(&u32::from(opcode).to_ne_bytes());

        *self.0.write_position += MessageHeader::WL_HEADER_SIZE as usize;
        Ok(())
    }
}

impl<'a> WlMessageWriter<'a> {
    pub fn new(buf: &'a mut [u8], fds: &'a mut FdBuffer, write_position: &'a mut usize) -> Self {
        let start = *write_position;
        Self {
            buf,
            write_position,
            start_position: start,
            fds,
            fd_count: 0,
        }
    }

    #[inline(always)]
    pub fn begin_message<T: WlObject>(
        self,
        op: T::Ops,
        type_id: u32,
    ) -> std::io::Result<WlMessageWriterGuard<'a>> {
        let mut guard = WlMessageWriterGuard(self);
        guard.begin::<T>(op, type_id)?;
        Ok(guard)
    }
}

impl Drop for WlMessageWriterGuard<'_> {
    fn drop(&mut self) {
        *self.0.write_position = self.0.start_position;
        self.0.fds.truncate_out(self.0.fd_count);
    }
}
