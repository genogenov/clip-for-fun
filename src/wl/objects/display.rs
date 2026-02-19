use std::ptr;

use crate::wl::objects::{MessageHeader, WLObject, wl_enum};

pub struct Display;

impl Display {
    pub const TYPE_ID: u32 = 1;

    pub fn parse_message(
        &self,
        header: &MessageHeader,
        buffer: &[u8],
        idx: usize,
    ) -> Option<DisplayEvent> {
        if header.object_id == Self::TYPE_ID && header.opcode == DisplayEvents::Error as u16 {
            let target_object_id =
                unsafe { ptr::read_unaligned(buffer.as_ptr().add(idx) as *const u32) };
            let error_code =
                unsafe { ptr::read_unaligned(buffer.as_ptr().add(idx + 4) as *const u32) };

            return Some(DisplayEvent::Error {
                target_object_id,
                error_code,
            });
        }

        None
    }
}

impl WLObject for Display {
    type Ops = DisplayOps;
    type Events = DisplayEvents;
}

wl_enum! {
    pub enum DisplayOps {
        Sync = 0,
        GetRegistry = 1,
    }
}

wl_enum! {
    pub enum DisplayEvents {
        Error = 0,
    }
}

#[repr(u16)]
#[derive(Debug)]
pub enum DisplayEvent {
    Error {
        target_object_id: u32,
        error_code: u32,
    },
}