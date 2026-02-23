use std::ptr;

use crate::wl::{objects::{MessageHeader, NoOps, WLObject, wl_enum}, wl_buffered_stream::WLBufferedStream};

pub struct WlDataControlDevice {
    pub local_id: u32,
}
impl WLObject for WlDataControlDevice {
    type Ops = WlDataControlDeviceOps;
    type Events = WlDataControlDeviceEvents;
}

wl_enum! {
    pub enum WlDataControlDeviceOps {
        SetSelection = 0,
    }
}

pub enum WlDataControlDeviceEvents {
    DataOffer = 0,
    Selection = 1,
    Finished = 2,
    PrimarySelection = 3,
}

pub enum DataControlDeviceEvent {
    DataOffer { new_id: u32 },
    Selection { offer_id: u32 },
    Finished,
    PrimarySelection { offer_id: u32 },
}

impl WlDataControlDevice {
    fn parse_message(
        &mut self,
        header: &MessageHeader,
        buffer: &[u8],
        idx: usize,
    ) -> Option<DataControlDeviceEvent> {
        if header.object_id != self.local_id {
            return None;
        }
        if header.opcode == WlDataControlDeviceEvents::DataOffer as u16 {
            let new_id = unsafe { ptr::read_unaligned(buffer.as_ptr().add(idx) as *const u32) };
            return Some(DataControlDeviceEvent::DataOffer { new_id });
        } else if header.opcode == WlDataControlDeviceEvents::Selection as u16 {
            let offer_id = unsafe { ptr::read_unaligned(buffer.as_ptr().add(idx) as *const u32) };
            return Some(DataControlDeviceEvent::Selection { offer_id });
        } else if header.opcode == WlDataControlDeviceEvents::Finished as u16 {
            return Some(DataControlDeviceEvent::Finished);
        } else if header.opcode == WlDataControlDeviceEvents::PrimarySelection as u16 {
            let offer_id = unsafe { ptr::read_unaligned(buffer.as_ptr().add(idx) as *const u32) };
            return Some(DataControlDeviceEvent::PrimarySelection { offer_id });
        }

        None
    }

    pub fn set_selection(&self, stream: &mut WLBufferedStream, source_id: u32) {
        let start = stream.begin_message::<WlDataControlDevice>(WlDataControlDeviceOps::SetSelection, self.local_id);
        stream.pack_u32(source_id);
        stream.end_message(start);
    }
}
