use std::ptr;

use crate::{
    unix_fd_stream::UnixFdStream,
    wl::objects::{MessageHeader, WLObject, WlStr},
};

pub struct WLBufferedStream {
    stream: UnixFdStream,
    write_buffer: [u8; 1024],
    write_cursor: usize,
    read_buffer: [u8; 4096],
    read_cursor: usize,
    bytes_read: usize,
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
        })
    }

    #[inline(always)]
    pub fn begin_read(&mut self) -> std::io::Result<()> {
        self.bytes_read = self.stream.read(&mut self.read_buffer)?;
        self.read_cursor = 0;
        Ok(())
    }

    pub fn read_next_message(&mut self) -> std::io::Result<Option<(MessageHeader, &[u8], usize)>> {
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
                return Ok(Some((header, &self.read_buffer, message_body_offset)));
            }

            // we may have read a partial message, so we need to move the remaining bytes to the beginning of the buffer
            let mut remaining_bytes = 0;
            if self.read_cursor < self.bytes_read {
                remaining_bytes = self.bytes_read - self.read_cursor;
                self.read_buffer
                    .copy_within(self.read_cursor..self.bytes_read, 0);
            }

            let new_bytes_read = self.stream.read(&mut self.read_buffer[remaining_bytes..])?; // Self::read(self.stream_fd, &mut self.read_buffer[remaining_bytes..], &mut self.in_fds[self.in_fd_count..])?; //self.stream.read(&mut self.read_buffer[remaining_bytes..])?;
            if new_bytes_read == 0 {
                break;
            }
            //println!("Read {} new bytes from the Wayland socket", new_bytes_read);
            self.bytes_read = remaining_bytes + new_bytes_read;
            self.read_cursor = 0;
        }

        Ok(None)
    }

    // pub fn dispatch_messages<F>(&mut self, mut handler: F) -> std::io::Result<()>
    // where
    //     F: FnMut(&MessageHeader, &[u8], usize),
    // {
    //     let mut bytes_read = self.stream.read(&mut self.read_buffer)?; //self.stream.read(&mut self.read_buffer)?;
    //     self.read_cursor = 0;
    //     let callback_id = self.current_object_id;

    //     while bytes_read > 0 {
    //         while (self.read_cursor + MessageHeader::WL_HEADER_SIZE as usize) <= bytes_read {
    //             let header_u64 = unsafe {
    //                 ptr::read_unaligned(
    //                     self.read_buffer.as_ptr().add(self.read_cursor) as *const u64
    //                 )
    //             };
    //             let header: MessageHeader = header_u64.into();

    //             if header.size > self.read_buffer.len() as u16
    //                 || header.size < MessageHeader::WL_HEADER_SIZE
    //             {
    //                 return Err(std::io::Error::new(
    //                     std::io::ErrorKind::Other,
    //                     format!("Message size {} invalid", header.size),
    //                 ));
    //             }
    //             if header.size as usize + self.read_cursor > bytes_read {
    //                 break;
    //             } else if header.object_id == callback_id
    //                 && header.opcode == WLCallbackEvents::Done as u16
    //             {
    //                 // println!("Received callback done event, registry enumeration complete");
    //                 return Ok(());
    //             } else if let Some(display_event) = self.display.parse_message(
    //                 &header,
    //                 &self.read_buffer,
    //                 self.read_cursor + MessageHeader::WL_HEADER_SIZE as usize,
    //             ) {
    //                 match display_event {
    //                     DisplayEvent::Error {
    //                         target_object_id,
    //                         error_code,
    //                     } => {
    //                         return Err(std::io::Error::new(
    //                             std::io::ErrorKind::Other,
    //                             format!(
    //                                 "Received error message from Wayland socket: target_object_id={}, error_code={}",
    //                                 target_object_id, error_code
    //                             ),
    //                         ));
    //                     }
    //                 }
    //             }

    //             handler(
    //                 &header,
    //                 &self.read_buffer,
    //                 self.read_cursor + MessageHeader::WL_HEADER_SIZE as usize,
    //             );

    //             self.read_cursor += header.size as usize;
    //             //sleep(Duration::from_secs(1));
    //         }

    //         // we may have read a partial message, so we need to move the remaining bytes to the beginning of the buffer
    //         let mut remaining_bytes = 0;
    //         if self.read_cursor < bytes_read {
    //             remaining_bytes = bytes_read - self.read_cursor;
    //             self.read_buffer
    //                 .copy_within(self.read_cursor..bytes_read, 0);
    //         }

    //         let new_bytes_read = self.stream.read(&mut self.read_buffer[remaining_bytes..])?; // Self::read(self.stream_fd, &mut self.read_buffer[remaining_bytes..], &mut self.in_fds[self.in_fd_count..])?; //self.stream.read(&mut self.read_buffer[remaining_bytes..])?;
    //         if new_bytes_read == 0 {
    //             break;
    //         }
    //         //println!("Read {} new bytes from the Wayland socket", new_bytes_read);
    //         bytes_read = remaining_bytes + new_bytes_read;
    //         self.read_cursor = 0;
    //     }

    //     Err(std::io::Error::new(
    //         std::io::ErrorKind::Other,
    //         "Failed to read all content from Wayland socket and did not find Done event",
    //     ))
    // }

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
}
