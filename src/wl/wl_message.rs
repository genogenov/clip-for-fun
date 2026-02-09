use crate::wl::wl_objects::WLObject;

pub struct WLMessage<T: WLObject> {
    pub opcode: T::Ops,
}