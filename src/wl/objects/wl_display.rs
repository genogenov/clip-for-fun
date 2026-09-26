use std::ptr;

use crate::{
    unix_fd_stream::WLFdBuffer,
    wl::{
        objects::{MessageHeader, WLCallbackEvents, WLObject, wl_enum, wl_registry::WlRegistry},
        wl_buffered_stream::WLBufferedStream,
        wl_message_reader::WlMessageReader,
    },
};

pub struct WlDisplay {
    callback_id: u32,
}

impl WlDisplay {
    pub const TYPE_ID: u32 = 1;

    pub fn new() -> Self {
        Self { callback_id: 0 }
    }

    fn parse_message(header: &MessageHeader, reader: &mut WlMessageReader) -> Option<DisplayEvent> {
        if header.object_id == Self::TYPE_ID && header.opcode == DisplayEvents::Error as u16 {
            let target_object_id = reader.u32()?;
            let error_code = reader.u32()?;

            let error_msg_slice = reader.str()?;
            let error_msg = String::from_utf8_lossy(error_msg_slice);

            return Some(DisplayEvent::Error {
                target_object_id,
                error_code,
                error_msg: error_msg.into_owned(),
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

    pub fn dispatch_messages<F>(
        &mut self,
        stream: &mut WLBufferedStream,
        mut handler: F,
    ) -> std::io::Result<()>
    where
        F: FnMut(&MessageHeader, &mut WlMessageReader, &mut WLFdBuffer),
    {
        while let Some((header, buffer, fds)) = stream.read_next_message()? {
            if header.object_id == self.callback_id
                && header.opcode == WLCallbackEvents::Done as u16
            {
                return Ok(());
            } else if let Some(display_event) =
                Self::parse_message(&header, &mut WlMessageReader::new(buffer))
            {
                match display_event {
                    DisplayEvent::Error {
                        target_object_id,
                        error_code,
                        error_msg,
                    } => {
                        return Err(std::io::Error::other(format!(
                            "Received error message from Wayland socket: target_object_id={}, error_code={}, message={}",
                            target_object_id, error_code, error_msg
                        )));
                    }
                }
            }

            let mut reader = WlMessageReader::new(buffer);
            handler(&header, &mut reader, fds);
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
