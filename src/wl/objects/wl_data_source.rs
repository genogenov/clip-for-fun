use crate::{
    unix_fd_stream::{WLFd, WLFdBuffer},
    wl::{
        objects::{WLObject, wl_enum},
        wl_buffered_stream::WLBufferedStream,
        wl_message_reader::WlMessageReader,
    },
};

pub struct WlDataControlSource {
    pub local_id: u32,
}

impl WLObject for WlDataControlSource {
    type Ops = WlDataControlSourceOps;
    type Events = WlDataControlSourceEvents;
}

pub enum WlDataControlSourceEvents {
    Send = 0,
    Cancelled = 1,
}

wl_enum! {
    pub enum WlDataControlSourceOps {
        Offer = 0,
        Destroy = 1,
    }
}

pub enum WlDataControlSourceEvent<'a> {
    Send { mime_type: &'a [u8], fd: WLFd },
    Cancelled,
}

impl WlDataControlSource {
    pub fn offer(&self, stream: &mut WLBufferedStream, mime_type: &str) {
        let bind_start = stream
            .begin_message::<WlDataControlSource>(WlDataControlSourceOps::Offer, self.local_id);
        stream.pack_str(mime_type);
        stream.end_message(bind_start);
    }

    pub fn parse_message<'a>(
        opcode: u16,
        buffer: &'a [u8],
        fds: &mut WLFdBuffer,
    ) -> std::io::Result<WlDataControlSourceEvent<'a>> {
        if opcode == WlDataControlSourceEvents::Send as u16 {
            let mut reader = WlMessageReader::new(buffer);
            let mime_type = reader
                .str()
                .ok_or_else(|| std::io::Error::other("Failed to read mime type"))?;
            return Ok(WlDataControlSourceEvent::Send {
                mime_type,
                fd: fds
                    .pop_last_in_fd()
                    .ok_or_else(|| std::io::Error::other("Failed to pop file descriptor"))?,
            });
        } else if opcode == WlDataControlSourceEvents::Cancelled as u16 {
            return Ok(WlDataControlSourceEvent::Cancelled);
        }
        Err(std::io::Error::other("Unknown opcode"))
    }
}
