use std::{marker::PhantomData, ptr};

use crate::wl::objects::{MessageHeader, WLObject, wl_enum, WlStr, wl_str_bytes};

#[derive(Debug)]
pub struct RegistryInterface<I>
where
    I: WLObject,
{
    pub global_name: u32,
    pub version: u32,
    pub interface_name: &'static WlStr,
    _marker: PhantomData<I>,
}

#[derive(Debug)]
pub struct WlSeat;
impl WLObject for WlSeat {
    type Ops = WlSeatOps;
    type Events = WlSeatEvents;
}
wl_enum! {
    pub enum WlSeatOps {
        GetPointer = 0,
        GetKeyboard = 1,
        GetTouch = 2,
        Release = 3,
    }
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

#[repr(u8)]
pub enum WLSeatCapability {
    Pointer = 1,
    Keyboard = 2,
    Touch = 4,
}

pub enum WlSeatEvents {
    Capabilities(WLSeatCapability),
}

pub enum NoEvents {}

type DataControlManager = RegistryInterface<ExtDataControlManagerV1>;
type ZwlrDataControlManager = RegistryInterface<ZwlrDataControlManagerV1>;
type DataDeviceManager = RegistryInterface<WlDataDeviceManager>;

wl_enum! {
    pub enum RegistryOps {
        Bind = 0,
    }
}

#[repr(u16)]
pub enum RegistryEvents {
    Global = 0,
}

#[derive(Debug)]
pub struct Registry {
    pub type_id: u32,
    pub data_device_manager: Option<DataDeviceManager>,
    pub ext_data_control_manager: Option<DataControlManager>,
    pub wl_seat: Option<RegistryInterface<WlSeat>>,
    pub zwlr_data_control_manager: Option<ZwlrDataControlManager>,
}

impl Registry {
    pub fn new(id: u32) -> Self {
        Self {
            type_id: id,
            data_device_manager: None,
            zwlr_data_control_manager: None,
            ext_data_control_manager: None,
            wl_seat: None,
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
                val if val == Registry::WL_DATA_DEVICE_MANAGER.bytes => {
                    let version = read_version(interface_name_len, idx, buffer)?;
                    self.data_device_manager = Some(RegistryInterface::<WlDataDeviceManager> {
                        global_name,
                        version,
                        interface_name: &Registry::WL_DATA_DEVICE_MANAGER,
                        _marker: PhantomData,
                    });
                    return Some(());
                }
                val if val == Registry::ZWLR_DATA_CONTROL_MANAGER_V1.bytes => {
                    let version = read_version(interface_name_len, idx, buffer)?;
                    self.zwlr_data_control_manager =
                        Some(RegistryInterface::<ZwlrDataControlManagerV1> {
                            global_name,
                            version,
                            interface_name: &Registry::ZWLR_DATA_CONTROL_MANAGER_V1,
                            _marker: PhantomData,
                        });
                    return Some(());
                }
                val if val == Registry::EXT_DATA_CONTROL_MANAGER_V1.bytes => {
                    let version = read_version(interface_name_len, idx, buffer)?;
                    self.ext_data_control_manager =
                        Some(RegistryInterface::<ExtDataControlManagerV1> {
                            global_name,
                            version,
                            interface_name: &Registry::EXT_DATA_CONTROL_MANAGER_V1,
                            _marker: PhantomData,
                        });
                    return Some(());
                }
                val if val == Registry::WL_SEAT.bytes => {
                    let version = read_version(interface_name_len, idx, buffer)?;
                    self.wl_seat = Some(RegistryInterface::<WlSeat> {
                        global_name,
                        version,
                        interface_name: &Registry::WL_SEAT,
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

impl Registry {
    pub const WL_DATA_DEVICE_MANAGER: WlStr = wl_str_bytes!("wl_data_device_manager");
    pub const ZWLR_DATA_CONTROL_MANAGER_V1: WlStr = wl_str_bytes!("zwlr_data_control_manager_v1");
    pub const EXT_DATA_CONTROL_MANAGER_V1: WlStr = wl_str_bytes!("ext_data_control_manager_v1");
    pub const WL_SEAT: WlStr = wl_str_bytes!("wl_seat");
}

impl WLObject for Registry {
    type Ops = RegistryOps;
    type Events = RegistryEvents;
}