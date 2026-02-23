use crate::wl::{objects::{NoEvents, WLObject, wl_data_control_device::WlDataControlDevice, wl_data_source::WlDataControlSource, wl_enum, wl_registry::{BoundInterface, RegistryInterface}}, wl_buffered_stream::WLBufferedStream};

pub trait DataDeviceManagerExt {
    fn manager_id(&self) -> u32;

    fn get_data_device(
        &self,
        stream: &mut WLBufferedStream,
        seat_id: u32,
    ) -> WlDataControlDevice {
        let data_device_start = stream.begin_message::<WlDataDeviceManager>(
            WlDataDeviceManagerOps::GetDataDevice,
            self.manager_id(),
        );
        let data_device_id = stream.pack_new_object_id();
        stream.pack_u32(seat_id);
        stream.end_message(data_device_start);

        WlDataControlDevice { local_id: data_device_id }
    }

    fn create_data_source(&self, stream: &mut WLBufferedStream) -> WlDataControlSource {
        let data_source_start = stream.begin_message::<WlDataDeviceManager>(
            WlDataDeviceManagerOps::CreateDataSource,
            self.manager_id(),
        );
        let data_source_id = stream.pack_new_object_id();
        stream.end_message(data_source_start);

        WlDataControlSource { local_id: data_source_id }
    }
}

#[derive(Debug, Clone, Copy)]
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

// #[repr(u8)]
// pub enum DragAndDrop {
//     None = 0,
//     Copy = 1,
//     Move = 2,
//     Ask = 4,
// }
pub enum WlDataDeviceManagerEvents {
    // DragAndDrop(DragAndDrop),
}

pub type DataControlManager = RegistryInterface<ExtDataControlManagerV1>;
pub type ZwlrDataControlManager = RegistryInterface<ZwlrDataControlManagerV1>;
pub type DataDeviceManager = RegistryInterface<WlDataDeviceManager>;


impl DataDeviceManagerExt for BoundInterface<ZwlrDataControlManagerV1> {
    fn manager_id(&self) -> u32 {
        self.local_id
    }
}

impl DataDeviceManagerExt for BoundInterface<ExtDataControlManagerV1> {
    fn manager_id(&self) -> u32 {
        self.local_id
    }
}