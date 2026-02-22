pub mod wl_registry;
pub mod wl_display;
pub mod wl_data_device;

pub trait WLObject {
    type Ops: Into<u16>;
    type Events;
}

pub enum NoEvents {}

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
pub(crate) use wl_enum;

#[repr(u16)]
pub enum WLCallbackEvents {
    Done = 0,
}

#[derive(Debug)]
pub struct WlStr {
    pub bytes: &'static [u8],
    pub str: &'static str,
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
        WlStr { str: S, bytes: &RESULT }
    }};
}
pub(crate) use wl_str_bytes;
