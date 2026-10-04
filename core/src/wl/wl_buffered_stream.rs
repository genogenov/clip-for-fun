use std::path::Path;

pub type NextMessageResult<'a> =
    std::io::Result<Option<(MessageHeader, &'a [u8], &'a mut FdBuffer)>>;

use crate::{
    log_debug,
    unix_fd_stream::{FdBuffer, UnixFdStream},
    wl::{objects::MessageHeader, wl_message_writer::WlMessageWriter},
};

pub struct WlBufferedStream {
    stream: UnixFdStream,
    write_buffer: [u8; 1024],
    write_cursor: usize,
    read_buffer: [u8; 4096],
    read_cursor: usize,
    bytes_read: usize,
    fd: FdBuffer,
}

impl WlBufferedStream {
    pub fn connect(socket_path: &Path) -> std::io::Result<Self> {
        Ok(Self::new(UnixFdStream::connect(socket_path)?))
    }

    pub(crate) fn new(stream: UnixFdStream) -> Self {
        Self {
            stream,
            write_buffer: [0u8; 1024],
            write_cursor: 0,
            read_buffer: [0u8; 4096],
            read_cursor: 0,
            bytes_read: 0,
            fd: FdBuffer::new(),
        }
    }

    pub fn read_next_message(&mut self) -> NextMessageResult<'_> {
        loop {
            if let Some(h) = self.read_buffer[self.read_cursor..self.bytes_read].first_chunk::<8>()
            {
                let header = MessageHeader::parse(h);

                if header.size > self.read_buffer.len() as u16
                    || header.size < MessageHeader::WL_HEADER_SIZE
                {
                    return Err(std::io::Error::other(format!(
                        "Message size {} invalid",
                        header.size
                    )));
                }
                // Only return the message once all of it is in the buffer; otherwise fall
                // through and read more bytes from the socket.
                if self.read_cursor + header.size as usize <= self.bytes_read {
                    let message_body_offset =
                        self.read_cursor + MessageHeader::WL_HEADER_SIZE as usize;
                    self.read_cursor += header.size as usize;
                    return Ok(Some((
                        header,
                        &self.read_buffer[message_body_offset..self.read_cursor],
                        &mut self.fd,
                    )));
                }
            }

            // we may have read a partial message, so we need to move the remaining bytes to the beginning of the buffer
            let mut remaining_bytes = 0;
            if self.read_cursor < self.bytes_read {
                remaining_bytes = self.bytes_read - self.read_cursor;
                self.read_buffer
                    .copy_within(self.read_cursor..self.bytes_read, 0);
            }

            let new_bytes_read = self
                .stream
                .read(&mut self.read_buffer[remaining_bytes..], &mut self.fd)?;
            if new_bytes_read == 0 {
                // EOF reached, no more messages to read.. if we have remaining bytes it means we have a partial message that we cant parse.
                if remaining_bytes > 0 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "EOF reached with partial message in buffer",
                    ));
                }
                return Ok(None);
            }
            self.bytes_read = remaining_bytes + new_bytes_read;
            self.read_cursor = 0;
        }
    }

    #[inline(always)]
    pub fn write(&mut self) -> std::io::Result<()> {
        if self.write_cursor == 0 {
            return Ok(());
        }
        log_debug!("Writing {} bytes to the stream", self.write_cursor);
        let result = self.stream.write(
            &self.write_buffer[..self.write_cursor],
            self.fd.peek_out_fds(),
        );
        self.write_cursor = 0;
        self.fd.clear_and_close_out_fds();
        result
    }

    pub fn get_writer<'a>(&'a mut self) -> WlMessageWriter<'a> {
        WlMessageWriter::new(&mut self.write_buffer, &mut self.fd, &mut self.write_cursor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        FdWriteAndClose,
        wl::objects::wl_display::{DisplayOps, WlDisplay},
    };
    use std::io::{Read, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::time::Duration;

    fn stream_pair() -> (WlBufferedStream, WlBufferedStream) {
        let (a, b) = UnixStream::pair().unwrap();
        (
            WlBufferedStream::new(a.into()),
            WlBufferedStream::new(b.into()),
        )
    }

    #[test]
    fn messages_round_trip_through_writer_and_reader() {
        let (mut tx, mut rx) = stream_pair();
        for (id, op) in [(3, DisplayOps::Sync), (4, DisplayOps::GetRegistry)] {
            let mut msg = tx.get_writer().begin_message::<WlDisplay>(op, id).unwrap();
            msg.pack_u32(42).unwrap();
            msg.end();
        }
        tx.write().unwrap();

        for (id, opcode) in [(3, 0), (4, 1)] {
            let (header, buf, _) = rx.read_next_message().unwrap().unwrap();
            assert_eq!(
                (header.object_id, header.opcode, header.size),
                (id, opcode, 12)
            );
            assert_eq!(u32::from_ne_bytes(buf[..4].try_into().unwrap()), 42);
        }
    }

    #[test]
    fn fds_arrive_in_order_and_sender_copies_are_closed() {
        let (mut tx, mut rx) = stream_pair();
        let (first_read, first_write) = UnixStream::pair().unwrap();
        let (second_read, second_write) = UnixStream::pair().unwrap();
        for fd in [first_write, second_write] {
            let mut msg = tx
                .get_writer()
                .begin_message::<WlDisplay>(DisplayOps::Sync, 7)
                .unwrap();
            msg.pack_fd(fd.into()).unwrap();
            msg.end();
        }
        tx.write().unwrap();

        let payloads: [&[u8]; 2] = [b"first", b"second"];
        for payload in payloads {
            let (header, _, fds) = rx.read_next_message().unwrap().unwrap();
            // fd arguments add no bytes to the message
            assert_eq!(
                (header.object_id, header.size),
                (7, MessageHeader::WL_HEADER_SIZE)
            );
            fds.pop_last_in_fd()
                .unwrap()
                .fd_write_and_close(payload)
                .unwrap();
        }

        for (mut reader, expected) in [first_read, second_read].into_iter().zip(payloads) {
            // Times out instead of hanging if tx kept its copy of the write end open.
            reader
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let mut got = Vec::new();
            reader.read_to_end(&mut got).unwrap();
            assert_eq!(got, expected);
        }
    }

    #[test]
    fn message_split_across_two_reads_is_returned() {
        let path = std::env::temp_dir().join(format!("cff-test-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).unwrap();
        let t = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            // 12-byte message: object_id=5, opcode=0, size=12, body u32=42
            let mut msg = Vec::new();
            msg.extend_from_slice(&5u32.to_ne_bytes());
            msg.extend_from_slice(&(12u32 << 16).to_ne_bytes());
            msg.extend_from_slice(&42u32.to_ne_bytes());
            s.write_all(&msg[..8]).unwrap(); // header only
            std::thread::sleep(std::time::Duration::from_millis(100));
            s.write_all(&msg[8..]).unwrap(); // body arrives later
            std::thread::sleep(std::time::Duration::from_millis(100));
        });
        let mut stream = WlBufferedStream::connect(&path).unwrap();
        let got = stream.read_next_message().unwrap();
        let ok = matches!(&got, Some((h, _, _)) if h.object_id == 5 && h.size == 12);
        t.join().unwrap();
        assert!(ok, "split message was not returned");
    }
}
