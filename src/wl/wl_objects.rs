use std::ptr;

pub trait WLObject {
    const TYPE_ID: u32;
    type Ops: Into<u16>;
    type Events;
}

#[derive(Debug)]
pub struct MessageHeader {
    pub object_id: u32,
    pub opcode: u16,
    pub size: u16,
}

impl MessageHeader {
    pub const WL_HEADER_SIZE: u16 = 8; // 4 bytes for object ID, 2 bytes for opcode, 2 bytes for message length

    pub fn deserialize(buffer: &[u8], idx: usize) -> Self {
        let object_id = unsafe { ptr::read(buffer.as_ptr().add(idx) as *const u32) };
        let opcode = unsafe { ptr::read(buffer.as_ptr().add(idx + 4) as *const u16) };
        let size = unsafe { ptr::read(buffer.as_ptr().add(idx + 6) as *const u16) };

        Self {
            object_id,
            opcode,
            size,
        }
    }
}

pub struct Display;

impl WLObject for Display {
    const TYPE_ID: u32 = 1;
    type Ops = DisplayOps;
    type Events = DisplayEvents;
}

macro_rules! wl_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $($variant:ident = $val:expr),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[repr(u16)]
        $vis enum $name {
            $($variant = $val),*
        }

        impl From<$name> for u16 {
            fn from(v: $name) -> u16 {
                v as u16
            }
        }
    };
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

pub struct Registry;

impl WLObject for Registry {
    const TYPE_ID: u32 = 2;
    type Ops = RegistryOps;
    type Events = RegistryEvents;
}

wl_enum! {
    pub enum RegistryOps {
        Bind = 0,
    }
}

#[repr(u16)]
pub enum RegistryEvents {
    Global = 0,
}

#[repr(u16)]
pub enum WLCallbackEvents {
    Done = 0,
}

#[repr(u8)]
#[derive(Debug)]
pub enum WlRegistryEvent<'a> {
    Global {
        global_name: u32,
        interface: &'a str,
        version: u32,
    },
}

impl Registry {
    pub const WL_DATA_DEVICE_MANAGER_NAME: &str = "wl_data_device_manager";

    pub fn try_parse_event<'a>(
        header: &MessageHeader,
        buffer: &'a [u8],
        idx: usize,
    ) -> Option<WlRegistryEvent<'a>> {
        if header.object_id == Self::TYPE_ID && header.opcode == RegistryEvents::Global as u16 {
            let global_name = unsafe { ptr::read(buffer.as_ptr().add(idx) as *const u32) };
            let interface_name_len =
                unsafe { ptr::read(buffer.as_ptr().add(idx + 4) as *const u32) };

            let interface_name =
                std::str::from_utf8(&buffer[idx + 8..idx + 8 + interface_name_len as usize])
                    .unwrap();

            let padded_len = (interface_name_len as usize + 3) & !3;
            let version = unsafe {
                ptr::read(
                    buffer
                        .as_ptr()
                        .add(idx + 8 + padded_len)
                        as *const u32,
                )
            };

            return Some(WlRegistryEvent::Global {
                global_name: global_name,
                interface: interface_name.trim_matches('\0'),
                version,
            });
        }

        None
    }
}

#[repr(u16)]
#[derive(Debug)]
pub enum DisplayEvent<'a> {
    Error {
        target_object_id: u32,
        error_code: u32,
        error_message: &'a str,
    },
}

impl Display {
    pub fn try_parse_event<'a>(
        header: &MessageHeader,
        buffer: &'a [u8],
        idx: usize,
    ) -> Option<DisplayEvent<'a>> {
        if header.object_id == Self::TYPE_ID && header.opcode == DisplayEvents::Error as u16 {
            let tarhet_object_id = unsafe { ptr::read(buffer.as_ptr().add(idx + 8) as *const u32) };
            let error_code = unsafe { ptr::read(buffer.as_ptr().add(idx + 12) as *const u32) };
            let error_message_len =
                unsafe { ptr::read(buffer.as_ptr().add(idx + 16) as *const u32) };
            let error_message =
                std::str::from_utf8(&buffer[idx + 20..idx + 20 + error_message_len as usize])
                    .unwrap();

            println!(
                "Received error message from Wayland socket: target object ID {}, error code {}, message {}",
                tarhet_object_id, error_code, error_message
            );

            return Some(DisplayEvent::Error {
                target_object_id: tarhet_object_id,
                error_code,
                error_message: error_message,
            });
        }

        None
    }
}
