use std::{os::fd::RawFd, ptr};

use crate::{
    unix_fd_stream::{UnixFdStream, WLFdBuffer},
    wl::objects::{MessageHeader, WLObject, WlStr},
};

pub struct WLBufferedStream {
    stream: UnixFdStream,
    write_buffer: [u8; 1024],
    write_cursor: usize,
    read_buffer: [u8; 4096],
    read_cursor: usize,
    bytes_read: usize,
    fd: WLFdBuffer,
    pub(crate) current_object_id: u32,
}

impl WLBufferedStream {
    pub fn connect(socket_path: &str) -> std::io::Result<Self> {
        let stream = UnixFdStream::connect(socket_path)?;
        Ok(Self {
            stream: stream,
            write_buffer: [0u8; 1024],
            write_cursor: 0,
            read_buffer: [0u8; 4096],
            read_cursor: 0,
            bytes_read: 0,
            current_object_id: 1,
            fd: WLFdBuffer::new(),
        })
    }

    #[inline(always)]
    pub fn begin_read(&mut self) -> std::io::Result<()> {
        self.bytes_read = self.stream.read(&mut self.read_buffer, &mut self.fd)?;
        self.read_cursor = 0;
        Ok(())
    }

    pub fn read_next_message(&mut self) -> std::io::Result<Option<(MessageHeader, &[u8], &mut WLFdBuffer, usize)>> {
        while self.bytes_read > 0 {
            while (self.read_cursor + MessageHeader::WL_HEADER_SIZE as usize) <= self.bytes_read {
                let header_u64 = unsafe {
                    ptr::read_unaligned(
                        self.read_buffer.as_ptr().add(self.read_cursor) as *const u64
                    )
                };
                let header: MessageHeader = header_u64.into();

                if header.size > self.read_buffer.len() as u16
                    || header.size < MessageHeader::WL_HEADER_SIZE
                {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Message size {} invalid", header.size),
                    ));
                }
                if header.size as usize + self.read_cursor > self.bytes_read {
                    break;
                }

                let message_body_offset = self.read_cursor + MessageHeader::WL_HEADER_SIZE as usize;
                self.read_cursor += header.size as usize;
                return Ok(Some((header, &self.read_buffer, &mut self.fd, message_body_offset)));
            }

            // we may have read a partial message, so we need to move the remaining bytes to the beginning of the buffer
            let mut remaining_bytes = 0;
            if self.read_cursor < self.bytes_read {
                remaining_bytes = self.bytes_read - self.read_cursor;
                self.read_buffer
                    .copy_within(self.read_cursor..self.bytes_read, 0);
            }

            let new_bytes_read = self.stream.read(&mut self.read_buffer[remaining_bytes..], &mut self.fd)?;
            if new_bytes_read == 0 {
                break;
            }
            self.bytes_read = remaining_bytes + new_bytes_read;
            self.read_cursor = 0;
        }

        Ok(None)
    }

    #[inline(always)]
    pub fn write(&mut self) -> std::io::Result<()> {
        self.stream.write(&self.write_buffer[..self.write_cursor])?;
        self.write_cursor = 0;
        Ok(())
    }

    #[inline(always)]
    pub fn pack_new_object_id(&mut self) -> u32 {
        self.current_object_id += 1;
        self.pack_u32(self.current_object_id);
        self.current_object_id
    }

    #[inline(always)]
    pub fn begin_message<T: WLObject>(&mut self, op: T::Ops, type_id: u32) -> usize {
        let opcode: u16 = op.into();

        let buf = &mut self.write_buffer[self.write_cursor..self.write_cursor + 12];
        buf[0..4].copy_from_slice(&type_id.to_ne_bytes());
        buf[4..6].copy_from_slice(&opcode.to_ne_bytes());

        let message_start = self.write_cursor;

        self.write_cursor += MessageHeader::WL_HEADER_SIZE as usize;
        message_start
    }

    #[inline(always)]
    pub fn pack_u32(&mut self, value: u32) {
        self.write_buffer[self.write_cursor..self.write_cursor + 4]
            .copy_from_slice(&value.to_ne_bytes());
        self.write_cursor += 4;
    }

    #[inline(always)]
    pub fn end_message(&mut self, message_start: usize) {
        let message_length = (self.write_cursor - message_start) as u16;
        self.write_buffer[message_start + 6..message_start + 8]
            .copy_from_slice(&message_length.to_ne_bytes());
    }

    #[inline(always)]
    pub fn pack_wl_str(&mut self, s: &WlStr) {
        // the bytes in wl_str are already prefixed with the length, and there is null terminator at the end, so we can just copy them directly to the write buffer
        let len = s.bytes.len() as u32;
        self.write_buffer[self.write_cursor..self.write_cursor + len as usize]
            .copy_from_slice(s.bytes);
        self.write_cursor += len as usize;
        // we also need to ensure the string is 4 byte aligned by adding padding if necessary
        let padding = (4 - (s.bytes.len() % 4)) % 4;
        for _ in 0..padding {
            self.write_buffer[self.write_cursor] = 0;
            self.write_cursor += 1;
        }
    }

    #[inline(always)]
    pub fn pack_str(&mut self, s: &str) {
        // we need to pack the string as len + bytes + null terminator and ensure it is 4 byte aligned. The len is the str bytes + the null terminator.
        let len = s.len() as u32 + 1;
        self.pack_u32(len);
        self.write_buffer[self.write_cursor..self.write_cursor + s.len()]
            .copy_from_slice(s.as_bytes());
        self.write_cursor += s.len();
        self.write_buffer[self.write_cursor] = 0; // null terminator
        self.write_cursor += 1; // move past null terminator
        // we also need to ensure the string is 4 byte aligned by adding padding if necessary
        let padding = (4 - (len % 4)) % 4;
        for _ in 0..padding {
            self.write_buffer[self.write_cursor] = 0;
            self.write_cursor += 1;
        }
    }
}
