use crate::wl::wl_objects::WLObject;

pub struct WLMessage {
    pub object_id: WLObject,
    pub opcode: u16,
}

impl WLMessage {
    pub fn new(object_id: WLObject, opcode: u16) -> Self {
        Self { object_id, opcode }
    }
}