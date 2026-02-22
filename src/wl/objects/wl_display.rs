use std::ptr;

use crate::wl::{
    objects::{MessageHeader, WLCallbackEvents, WLObject, wl_enum, wl_registry::WlRegistry},
    wl_buffered_stream::WLBufferedStream,
};

pub struct WlDisplay{
    callback_id: u32,
}

impl WlDisplay {
    pub const TYPE_ID: u32 = 1;

    pub fn new() -> Self {
        Self {
            callback_id: 0,
        }
    }

    fn parse_message(
        header: &MessageHeader,
        buffer: &[u8],
        idx: usize,
    ) -> Option<DisplayEvent> {
        if header.object_id == Self::TYPE_ID && header.opcode == DisplayEvents::Error as u16 {
            let target_object_id =
                unsafe { ptr::read_unaligned(buffer.as_ptr().add(idx) as *const u32) };
            let error_code =
                unsafe { ptr::read_unaligned(buffer.as_ptr().add(idx + 4) as *const u32) };
                
                // read error msg from buffer
                let error_msg_len =
                    unsafe { ptr::read_unaligned(buffer.as_ptr().add(idx + 8) as *const u32) } as usize;
                let error_msg_start = idx + 12;
                let error_msg_end = error_msg_start + error_msg_len;
                if error_msg_end > buffer.len() {
                    return None; // Not enough data for error message
                }
                let error_msg_slice = &buffer[error_msg_start..error_msg_end];
                let error_msg = std::str::from_utf8(error_msg_slice).unwrap_or("<invalid utf-8>").to_string();

            return Some(DisplayEvent::Error {
                target_object_id,
                error_code,
                error_msg,
            });
        }

        None
    }

    pub fn get_registry(&mut self, stream: &mut WLBufferedStream) -> std::io::Result<WlRegistry> {
        let registry_start =
            stream.begin_message::<WlDisplay>(DisplayOps::GetRegistry, WlDisplay::TYPE_ID);
        let registry_id = stream.pack_new_object_id();
        stream.end_message(registry_start);

        Ok(WlRegistry::new(registry_id))
    }

    pub fn roundtrip_sync(&mut self, stream: &mut WLBufferedStream) -> std::io::Result<()> {
        let sync_start = stream.begin_message::<WlDisplay>(DisplayOps::Sync, WlDisplay::TYPE_ID);
        self.callback_id = stream.pack_new_object_id();
        stream.end_message(sync_start);

        stream.write()?;
        stream.begin_read()
    }

    pub fn dispatch_messages<F>(&mut self, stream: &mut WLBufferedStream, mut handler: F) -> std::io::Result<()>
    where
        F: FnMut(&MessageHeader, &[u8], usize),
    {
        while let Some((header, buffer, idx)) = stream.read_next_message()? {
            if header.object_id == self.callback_id && header.opcode == WLCallbackEvents::Done as u16 {
                // println!("Received callback done event, registry enumeration complete");
                return Ok(());
            } else if let Some(display_event) = Self::parse_message(
                &header,
                &buffer,
                idx,
            ) {
                match display_event {
                    DisplayEvent::Error {
                        target_object_id,
                        error_code,
                        error_msg
                    } => {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            format!(
                                "Received error message from Wayland socket: target_object_id={}, error_code={}, message={}",
                                target_object_id, error_code, error_msg
                            ),
                        ));
                    }
                }
            }

            handler(
                &header,
                &buffer,
                idx,
            );
        }

        Ok(())
    }
}

impl WLObject for WlDisplay {
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
        error_msg: String,
    },
}
