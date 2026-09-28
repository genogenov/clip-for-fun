use crate::wl::{
    objects::{WlObject, wl_enum, wl_registry::WlRegistry},
    wl_buffered_stream::WlBufferedStream,
    wl_message_reader::WlMessageReader,
    wl_message_router::{WlInterface, WlMessageRouter},
};

pub struct WlDisplay {
    callback_id: u32,
}

impl Default for WlDisplay {
    fn default() -> Self {
        Self::new()
    }
}

impl WlDisplay {
    pub const TYPE_ID: u32 = 1;

    pub fn new() -> Self {
        Self { callback_id: 0 }
    }

    pub fn parse_message(opcode: u16, buffer: &[u8]) -> Option<DisplayEvent> {
        match opcode {
            DISPLAYEVENT_ERROR => {
                let mut reader = WlMessageReader::new(buffer);
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
            DISPLAYEVENT_DELETEID => {
                let mut reader = WlMessageReader::new(buffer);
                let id = reader.u32()?;

                return Some(DisplayEvent::DeleteId { id });
            }
            _ => {}
        }
        None
    }

    pub fn get_registry(
        &mut self,
        stream: &mut WlBufferedStream,
        router: &mut WlMessageRouter,
    ) -> std::io::Result<WlRegistry> {
        let registry_start =
            stream.begin_message::<WlDisplay>(DisplayOps::GetRegistry, WlDisplay::TYPE_ID);
        let registry_id = stream.pack_new_object_id(router, WlInterface::Registry)?;
        stream.end_message(registry_start);

        Ok(WlRegistry::new(registry_id))
    }

    pub fn sync(
        &mut self,
        stream: &mut WlBufferedStream,
        router: &mut WlMessageRouter,
    ) -> std::io::Result<()> {
        let sync_start = stream.begin_message::<WlDisplay>(DisplayOps::Sync, WlDisplay::TYPE_ID);
        self.callback_id = stream.pack_new_object_id(router, WlInterface::Callback)?;
        stream.end_message(sync_start);

        stream.write()
    }
}

impl WlObject for WlDisplay {
    type Ops = DisplayOps;
    type Events = DisplayEvents;
    const VERSION: u32 = 1;
}

wl_enum! {
    pub enum DisplayOps {
        Sync = 0,
        GetRegistry = 1,
    }
}

wl_enum! {
    pub enum DisplayEvents {
        Error = DISPLAYEVENT_ERROR,
        DeleteId = DISPLAYEVENT_DELETEID,
    }
}

pub const DISPLAYEVENT_ERROR: u16 = 0;
pub const DISPLAYEVENT_DELETEID: u16 = 1;

#[repr(u16)]
#[derive(Debug)]
pub enum DisplayEvent {
    Error {
        target_object_id: u32,
        error_code: u32,
        error_msg: String,
    },
    DeleteId {
        id: u32,
    },
}
