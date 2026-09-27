use crate::wl::{
    debug_println, objects::{
        MessageHeader, WLCallbackEvents,
        wl_data_source::{WlDataControlSource, WlDataControlSourceEvent},
        wl_display::{DisplayEvent, WlDisplay},
    }, wl_buffered_stream::WLBufferedStream,
};
use std::{
    io::{Error, ErrorKind, Result},
    ops::ControlFlow,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WlInterface {
    Display,
    Callback,
    Registry,
    Seat,
    ExtDataControlManager,
    ZwlrDataControlManager,
    DataDeviceManager,
    DataControlDevice,
    DataControlSource,
    DataControlOffer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Free,
    Live(WlInterface),
    Reserved,
}

pub enum WLEvent<'a> {
    DataControlSource(WlDataControlSourceEvent<'a>),
    Registry(MessageHeader, &'a [u8]),

    SyncDone,

    Ignored,
}

const MAX_REGISTERED_INTERFACES: usize = 16;

pub struct WlMessageRouter {
    client_interfaces: [Slot; MAX_REGISTERED_INTERFACES],
    server_interfaces: [Slot; MAX_REGISTERED_INTERFACES],
}

impl WlMessageRouter {
    pub fn new() -> Self {
        let mut instance = Self {
            client_interfaces: [Slot::Free; MAX_REGISTERED_INTERFACES],
            server_interfaces: [Slot::Free; MAX_REGISTERED_INTERFACES],
        };
        instance.client_interfaces[0] = Slot::Reserved;
        instance.client_interfaces[1] = Slot::Live(WlInterface::Display);
        instance
    }

    pub fn register(&mut self, interface: WlInterface) -> Result<u32> {
        if let Some(free_index) = self.find_free_client() {
            self.client_interfaces[free_index] = Slot::Live(interface);
            debug_println!("Registered interface {:?} at client slot {}", interface, free_index);
            Ok(free_index as u32)
        } else {
            Err(Error::other("No free client slot available"))
        }
    }

    fn next_event<'a>(
        &mut self,
        stream: &'a mut WLBufferedStream,
    ) -> std::io::Result<Option<WLEvent<'a>>> {
        let Some((header, buffer, fds)) = stream.read_next_message()? else {
            return Ok(None);
        };
        {
            let interface = match self.lookup_slot(header.object_id) {
                Some(interface) => interface,
                // compositor-created object we don't track (e.g. an offer)
                None if header.object_id >= 0xff00_0000 => return Ok(Some(WLEvent::Ignored)),
                None => return Err(Error::other("No slot found")),
            };

            match interface {
                WlInterface::Callback => {
                    if header.opcode == WLCallbackEvents::Done as u16 {
                        return Ok(Some(WLEvent::SyncDone));
                    }
                    Err(Error::other("Unknown opcode"))
                }
                WlInterface::Display => {
                    let Some(display_event) = WlDisplay::parse_message(header.opcode, buffer)
                    else {
                        return Ok(Some(WLEvent::Ignored));
                    };
                    match display_event {
                        DisplayEvent::Error {
                            target_object_id,
                            error_code,
                            error_msg,
                        } => Err(std::io::Error::other(format!(
                            "Received error message from Wayland socket: target_object_id={}, error_code={}, message={}",
                            target_object_id, error_code, error_msg
                        ))),
                        DisplayEvent::DeleteId { id } => {
                            debug_println!("Deleting WL client object id: {}", id);
                            if let Some(slot) = self.client_interfaces.get_mut(id as usize) {
                                *slot = Slot::Free;
                            }
                            Ok(Some(WLEvent::Ignored))
                        }
                    }
                }
                WlInterface::Seat => Ok(Some(WLEvent::Ignored)),
                WlInterface::ExtDataControlManager => Ok(Some(WLEvent::Ignored)),
                WlInterface::ZwlrDataControlManager => Ok(Some(WLEvent::Ignored)),
                WlInterface::DataDeviceManager => Ok(Some(WLEvent::Ignored)),
                WlInterface::DataControlDevice => Ok(Some(WLEvent::Ignored)),
                WlInterface::DataControlSource => Ok(Some(WLEvent::DataControlSource(
                    WlDataControlSource::parse_message(header.opcode, buffer, fds)?,
                ))),
                WlInterface::DataControlOffer => Ok(Some(WLEvent::Ignored)),
                WlInterface::Registry => Ok(Some(WLEvent::Registry(header, buffer))),
            }
        }
    }

    pub fn dispatch_messages<F: FnMut(WLEvent<'_>) -> ControlFlow<()>>(
        &mut self,
        stream: &mut WLBufferedStream,
        mut fnhandler: F,
    ) -> std::io::Result<()> {
        while let Some(event) = self.next_event(stream)? {
            if fnhandler(event).is_break() {
                return Ok(());
            }
        }

        Err(std::io::Error::new(
            ErrorKind::UnexpectedEof,
            "Unexpected end of stream",
        ))
    }

    fn find_free_client(&self) -> Option<usize> {
        self.client_interfaces
            .iter()
            .position(|slot| *slot == Slot::Free)
    }

    fn lookup_slot(&self, object_id: u32) -> Option<WlInterface> {
        const SERVER_ID_START: u32 = 0xff000000;
        let slot = if object_id >= SERVER_ID_START {
            self.server_interfaces
                .get((object_id - SERVER_ID_START) as usize)
        } else {
            self.client_interfaces.get(object_id as usize)
        };
        match slot {
            Some(Slot::Live(interface)) => Some(*interface),
            Some(Slot::Free) => None,
            Some(Slot::Reserved) => None,
            None => None,
        }
    }
}
