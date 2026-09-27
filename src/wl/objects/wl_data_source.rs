use std::ptr;

use crate::{
    unix_fd_stream::{WLFd, WLFdBuffer},
    wl::{
        objects::{MessageHeader, WLObject, wl_enum},
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
        &self,
        header: &MessageHeader,
        reader: &'a mut WlMessageReader,
        fds: &mut WLFdBuffer,
    ) -> Option<WlDataControlSourceEvent<'a>> {
        if header.object_id != self.local_id {
            return None;
        }
        if header.opcode == WlDataControlSourceEvents::Send as u16 {
            // alloc... think about a more performant way to do this without allocation, maybe preallocated str buffers.
            let mime_type = reader.str()?;
            return Some(WlDataControlSourceEvent::Send {
                mime_type,
                fd: fds.pop_last_in_fd().unwrap(),
            });
        } else if header.opcode == WlDataControlSourceEvents::Cancelled as u16 {
            return Some(WlDataControlSourceEvent::Cancelled);
        }
        None
    }
}
