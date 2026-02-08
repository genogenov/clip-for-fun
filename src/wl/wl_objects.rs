#[repr(u32)]
pub enum WLObject {
    Display = 1u32,
    Registry = 2u32,
}

pub const WL_HEADER_SIZE: u16 = 8; // 4 bytes for object ID, 2 bytes for opcode, 2 bytes for message length

pub const WL_DISPLAY_OP_SYNC: u16 = 0;
pub const WL_DISPLAY_OP_GET_REGISTRY: u16 = 1;
pub const WL_DISPLAY_EV_ERROR: u16 = 0;

pub const WL_REGISTRY_EV_GLOBAL: u16 = 0;
pub const WL_REGISTRY_CALLBACK_DONE: u16 = 0;

pub const WL_DATA_DEVICE_MANAGER_NAME: &str = "wl_data_device_manager";