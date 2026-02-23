use std::{os::fd::RawFd, ptr};

use crate::{unix_fd_stream::WLFdBuffer, wl::{objects::{MessageHeader, WLObject, wl_enum}, wl_buffered_stream::WLBufferedStream}};

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

pub enum WlDataControlSourceEvent {
    Send { mime_type: String, fd: RawFd },
    Cancelled,
}

impl WlDataControlSource {
    pub fn offer(&self, stream: &mut WLBufferedStream, mime_type: &str) {
        let bind_start = stream.begin_message::<WlDataControlSource>(WlDataControlSourceOps::Offer, self.local_id);
        stream.pack_str(mime_type);
        stream.end_message(bind_start);
    }

    pub fn parse_message(
        &mut self,
        header: &MessageHeader,
        buffer: &[u8],
        fds: &mut WLFdBuffer,
        idx: usize,
    ) -> Option<WlDataControlSourceEvent> {
        if header.object_id != self.local_id {
            return None;
        }
        if header.opcode == WlDataControlSourceEvents::Send as u16 {
            let mime_type_len =
                unsafe { ptr::read_unaligned(buffer.as_ptr().add(idx) as *const u32) };
            let mime_type_end = idx + 4 + mime_type_len as usize;
            if mime_type_end > buffer.len() {
                return None; // Not enough data
            }
            //let interface_length_name_slice = &buffer[idx..mime_type_end];

            // alloc... think about a more performant way to do this without allocation, maybe preallocated str buffers.
            //let mime_type: String = String::from_utf8_lossy(interface_length_name_slice).into_owned();
            return Some(WlDataControlSourceEvent::Send { mime_type: String::new(), fd: fds.pop_last_in_fd().unwrap() });
        } else if header.opcode == WlDataControlSourceEvents::Cancelled as u16 {
            return Some(WlDataControlSourceEvent::Cancelled);
        }
        None
    }
}
