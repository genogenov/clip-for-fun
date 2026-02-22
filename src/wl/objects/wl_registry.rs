use std::fmt::Debug;
use std::{marker::PhantomData, ptr};

use crate::wl::objects::wl_data_device::{DataControlManager, DataDeviceManager, ExtDataControlManagerV1, WlDataDeviceManager, ZwlrDataControlManager, ZwlrDataControlManagerV1};
use crate::wl::{
    objects::{MessageHeader, WLObject, WlStr, wl_enum, wl_str_bytes},
    wl_buffered_stream::WLBufferedStream,
};

#[derive(Debug, Clone)]
pub struct RegistryInterface<I>
where
    I: WLObject,
{
    pub global_name: u32,
    pub version: u32,
    pub interface_name: &'static WlStr,
    _marker: PhantomData<I>,
}

pub struct BoundInterface<I>
where
    I: WLObject,
{
    pub local_id: u32,
    pub interface: RegistryInterface<I>,
    _marker: PhantomData<I>,
}

impl<I: WLObject> BoundInterface<I>  {
    pub fn new(local_id: u32, interface: RegistryInterface<I>) -> Self {
        Self {
            local_id,
            interface,
            _marker: PhantomData,
        }
    }
}

impl<I: WLObject> WLObject for BoundInterface<I> {
    type Ops = I::Ops;
    type Events = I::Events;
}

#[derive(Debug, Clone)]
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


// #[repr(u8)]
// pub enum WLSeatCapability {
//     Pointer = 1,
//     Keyboard = 2,
//     Touch = 4,
// }

pub enum WlSeatEvents {
    // Capabilities(WLSeatCapability),
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

pub struct WlRegistry {
    type_id: u32,
    pub data_device_manager: Option<DataDeviceManager>,
    pub ext_data_control_manager: Option<DataControlManager>,
    pub wl_seat: Option<RegistryInterface<WlSeat>>,
    pub zwlr_data_control_manager: Option<ZwlrDataControlManager>,
}

impl Debug for WlRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WlRegistry")
            .field("type_id", &self.type_id)
            .field("data_device_manager", &self.data_device_manager)
            .field("ext_data_control_manager", &self.ext_data_control_manager)
            .field("wl_seat", &self.wl_seat)
            .field("zwlr_data_control_manager", &self.zwlr_data_control_manager)
            .finish()
    }
}

impl WlRegistry {
    pub fn new(id: u32) -> Self {
        Self {
            type_id: id,
            data_device_manager: None,
            zwlr_data_control_manager: None,
            ext_data_control_manager: None,
            wl_seat: None,
        }
    }

    pub fn bind<I>(
        &self,
        stream: &mut WLBufferedStream,
        interface: RegistryInterface<I>,
    ) -> std::io::Result<BoundInterface<I>>
    where
        I: WLObject,
    {
        let bind_start = stream.begin_message::<WlRegistry>(RegistryOps::Bind, self.type_id);
        stream.pack_u32(interface.global_name);
        stream.pack_wl_str(interface.interface_name);
        stream.pack_u32(interface.version);
        let binding_id = stream.pack_new_object_id();
        stream.end_message(bind_start);
        Ok(BoundInterface::new(binding_id, interface))
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
                val if val == WlRegistry::WL_DATA_DEVICE_MANAGER.bytes => {
                    let version = read_version(interface_name_len, idx, buffer)?;
                    self.data_device_manager = Some(RegistryInterface::<WlDataDeviceManager> {
                        global_name,
                        version,
                        interface_name: &WlRegistry::WL_DATA_DEVICE_MANAGER,
                        _marker: PhantomData,
                    });
                    return Some(());
                }
                val if val == WlRegistry::ZWLR_DATA_CONTROL_MANAGER_V1.bytes => {
                    let version = read_version(interface_name_len, idx, buffer)?;
                    self.zwlr_data_control_manager =
                        Some(RegistryInterface::<ZwlrDataControlManagerV1> {
                            global_name,
                            version,
                            interface_name: &WlRegistry::ZWLR_DATA_CONTROL_MANAGER_V1,
                            _marker: PhantomData,
                        });
                    return Some(());
                }
                val if val == WlRegistry::EXT_DATA_CONTROL_MANAGER_V1.bytes => {
                    let version = read_version(interface_name_len, idx, buffer)?;
                    self.ext_data_control_manager =
                        Some(RegistryInterface::<ExtDataControlManagerV1> {
                            global_name,
                            version,
                            interface_name: &WlRegistry::EXT_DATA_CONTROL_MANAGER_V1,
                            _marker: PhantomData,
                        });
                    return Some(());
                }
                val if val == WlRegistry::WL_SEAT.bytes => {
                    let version = read_version(interface_name_len, idx, buffer)?;
                    self.wl_seat = Some(RegistryInterface::<WlSeat> {
                        global_name,
                        version,
                        interface_name: &WlRegistry::WL_SEAT,
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

impl WlRegistry {
    const WL_DATA_DEVICE_MANAGER: WlStr = wl_str_bytes!("wl_data_device_manager");
    const ZWLR_DATA_CONTROL_MANAGER_V1: WlStr = wl_str_bytes!("zwlr_data_control_manager_v1");
    const EXT_DATA_CONTROL_MANAGER_V1: WlStr = wl_str_bytes!("ext_data_control_manager_v1");
    const WL_SEAT: WlStr = wl_str_bytes!("wl_seat");
}

impl WLObject for WlRegistry {
    type Ops = RegistryOps;
    type Events = RegistryEvents;
}
