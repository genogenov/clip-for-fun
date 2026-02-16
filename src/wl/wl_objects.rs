use std::{
    marker::PhantomData,
    ptr::{self},
};

pub trait WLObject {
    type Ops: Into<u16>;
    type Events;
}

#[derive(Debug)]
pub struct MessageHeader {
    pub object_id: u32,
    pub opcode: u16,
    pub size: u16,
}

impl From<u64> for MessageHeader {
    fn from(value: u64) -> Self {
        let bytes: [u8; 8] = value.to_ne_bytes();
        Self {
            object_id: u32::from_ne_bytes(bytes[0..4].try_into().unwrap()),
            opcode: u16::from_ne_bytes(bytes[4..6].try_into().unwrap()),
            size: u16::from_ne_bytes(bytes[6..8].try_into().unwrap()),
        }
    }
}

impl MessageHeader {
    pub const WL_HEADER_SIZE: u16 = 8; // 4 bytes for object ID, 2 bytes for opcode, 2 bytes for message length
}

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

#[derive(Debug)]
pub struct RegistryInterface<I>
where
    I: WLObject,
{
    global_name: u32,
    version: u32,
    _marker: PhantomData<I>,
}

#[derive(Debug)]
pub struct ExtDataControlManagerV1;
impl WLObject for ExtDataControlManagerV1 {
    type Ops = ExtDataControlManagerOps;
    type Events = NoEvents;
}
wl_enum! {
    pub enum ExtDataControlManagerOps {
        CreateDataSource = 0,
        GetDataDevice = 1,
        Destroy = 2,
    }
}

#[derive(Debug)]
pub struct ZwlrDataControlManagerV1;
impl WLObject for ZwlrDataControlManagerV1 {
    type Ops = ZwlrDataControlManagerOps;
    type Events = NoEvents;
}
wl_enum! {
    pub enum ZwlrDataControlManagerOps {
        CreateDataSource = 0,
        GetDataDevice = 1,
        Destroy = 2,
    }
}

#[derive(Debug)]
pub struct WlDataDeviceManager;
impl WLObject for WlDataDeviceManager {
    type Ops = WlDataDeviceManagerOps;
    type Events = WlDataDeviceManagerEvents;
}
wl_enum! {
    pub enum WlDataDeviceManagerOps {
        CreateDataSource = 0,
        GetDataDevice = 1,
    }
}

#[repr(u8)]
pub enum DragAndDrop {
    None = 0,
    Copy = 1,
    Move = 2,
    Ask = 4,
}
pub enum WlDataDeviceManagerEvents {
    DragAndDrop(DragAndDrop),
}

pub enum NoEvents {}

type DataControlManager = RegistryInterface<ExtDataControlManagerV1>;
type ZwlrDataControlManager = RegistryInterface<ZwlrDataControlManagerV1>;
type DataDeviceManager = RegistryInterface<WlDataDeviceManager>;

#[derive(Debug)]
pub struct Registry {
    pub type_id: u32,
    pub data_device_manager: Option<DataDeviceManager>,
    pub ext_data_control_manager: Option<DataControlManager>,
    pub zwlr_data_control_manager: Option<ZwlrDataControlManager>,
}

impl Registry {
    pub fn new(id: u32) -> Self {
        Self {
            type_id: id,
            data_device_manager: None,
            zwlr_data_control_manager: None,
            ext_data_control_manager: None,
        }
    }

    pub fn add_interface(
        &mut self,
        header: &MessageHeader,
        buffer: &[u8],
        idx: usize,
    ) -> Option<()> {
        if header.object_id == self.type_id && header.opcode == RegistryEvents::Global as u16 {
            let global_name =
                unsafe { ptr::read_unaligned(buffer.as_ptr().add(idx) as *const u32) };
            let interface_name_len =
                unsafe { ptr::read_unaligned(buffer.as_ptr().add(idx + 4) as *const u32) };

            let interface_name_end = idx + 8 + interface_name_len as usize;
            if interface_name_end > buffer.len() {
                return None; // Not enough data for interface name
            }
            let interface_length_name_slice = &buffer[idx + 4..interface_name_end];

            #[inline(always)]
            fn read_version(interface_name_len: u32, idx: usize, buffer: &[u8]) -> Option<u32> {
                let padded_len = (interface_name_len as usize + 3) & !3;
                let version_offset = idx + 8 + padded_len;
                if version_offset + 4 > buffer.len() {
                    return None;
                }
                unsafe {
                    Some(ptr::read_unaligned(
                        buffer.as_ptr().add(version_offset) as *const u32
                    ))
                }
            }

            match interface_length_name_slice {
                val if val == Registry::WL_DATA_DEVICE_MANAGER.1 => {
                    let version = read_version(interface_name_len, idx, buffer)?;
                    self.data_device_manager = Some(RegistryInterface::<WlDataDeviceManager> {
                        global_name,
                        version,
                        _marker: PhantomData,
                    });
                    return Some(());
                }
                val if val == Registry::ZWLR_DATA_CONTROL_MANAGER_V1.1 => {
                    let version = read_version(interface_name_len, idx, buffer)?;
                    self.zwlr_data_control_manager = Some(RegistryInterface::<ZwlrDataControlManagerV1> {
                        global_name,
                        version,
                        _marker: PhantomData,
                    });
                    return Some(());
                }
                val if val == Registry::EXT_DATA_CONTROL_MANAGER_V1.1 => {
                    let version = read_version(interface_name_len, idx, buffer)?;
                    self.ext_data_control_manager = Some(RegistryInterface::<ExtDataControlManagerV1> {
                        global_name,
                        version,
                        _marker: PhantomData,
                    });
                    return Some(());
                }
                // Add more interfaces here as needed
                _ => return None,
            };
        }

        None
    }
}

impl WLObject for Registry {
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

macro_rules! wl_str_bytes {
    ($s:expr) => {{
        const S: &str = $s;
        const LEN: usize = 4 + S.len() + 1; // 4 bytes for length prefix, string bytes, and null terminator
        const RESULT: [u8; LEN] = {
            let size = ((S.len() + 1) as u32).to_ne_bytes();
            let b = S.as_bytes();
            let mut r = [0u8; LEN];
            r[0] = size[0]; r[1] = size[1];
            r[2] = size[2]; r[3] = size[3];
            r[4 + S.len()] = 0; // null terminator
            let mut i = 0;
            while i < b.len() {
                r[i + 4] = b[i];
                i += 1;
            }
            r
        };
        ($s, &RESULT)
    }};
}

impl Registry {
    pub const WL_DATA_DEVICE_MANAGER: (&str, &[u8]) = wl_str_bytes!("wl_data_device_manager");
    pub const ZWLR_DATA_CONTROL_MANAGER_V1: (&str, &[u8]) =
        wl_str_bytes!("zwlr_data_control_manager_v1");
    pub const EXT_DATA_CONTROL_MANAGER_V1: (&str, &[u8]) =
        wl_str_bytes!("ext_data_control_manager_v1");
}

#[repr(u16)]
#[derive(Debug)]
pub enum DisplayEvent {
    Error {
        target_object_id: u32,
        error_code: u32,
    },
}
