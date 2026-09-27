use crate::{
    unix_fd_stream::WLFdBuffer,
    wl::{
        objects::{
            MessageHeader, WLCallbackEvents,
            wl_display::{DisplayEvent, WlDisplay},
        },
        wl_buffered_stream::WLBufferedStream,
        wl_message_reader::WlMessageReader,
    },
};
use std::io::{Error, ErrorKind::Other, Result};

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
            Ok(free_index as u32)
        } else {
            Err(Error::other("No free client slot available"))
        }
    }

    pub fn dispatch_messages<F>(
        &mut self,
        stream: &mut WLBufferedStream,
        mut handler: F,
    ) -> std::io::Result<()>
    where
        F: FnMut(&MessageHeader, &mut WlMessageReader, &mut WLFdBuffer),
    {
        while let Some((header, buffer, fds)) = stream.read_next_message()? {
            let interface = self
                .lookup_slot(header.object_id)
                .ok_or_else(|| Error::other("No slot found"))?;

            match interface {
                WlInterface::Callback => {
                    if header.opcode == WLCallbackEvents::Done as u16 {
                        return Ok(());
                    }
                }
                WlInterface::Display => {
                    let Some(display_event) =
                        WlDisplay::parse_message(&header, &mut WlMessageReader::new(buffer))
                    else {
                        continue;
                    };
                    match display_event {
                        DisplayEvent::Error {
                            target_object_id,
                            error_code,
                            error_msg,
                        } => {
                            return Err(std::io::Error::other(format!(
                                "Received error message from Wayland socket: target_object_id={}, error_code={}, message={}",
                                target_object_id, error_code, error_msg
                            )));
                        }
                    }
                }
                WlInterface::Seat => {
                    // Handle Seat interface messages
                }
                WlInterface::ExtDataControlManager => {
                    // Handle ExtDataControlManager interface messages
                }
                WlInterface::ZwlrDataControlManager => {
                    // Handle ZwlrDataControlManager interface messages
                }
                WlInterface::DataDeviceManager => {
                    // Handle DataDeviceManager interface messages
                }
                WlInterface::DataControlDevice => {
                    // Handle DataControlDevice interface messages
                }
                WlInterface::DataControlSource => {
                    // Handle DataControlSource interface messages
                }
                WlInterface::DataControlOffer => {
                    // Handle DataControlOffer interface messages
                }
                WlInterface::Registry => {
                    // Handle Registry interface messages
                }
            }

            let mut reader = WlMessageReader::new(buffer);
            handler(&header, &mut reader, fds);
        }

        Ok(())
    }

    fn find_free_client(&self) -> Option<usize> {
        self.client_interfaces
            .iter()
            .position(|slot| *slot == Slot::Free)
    }

    fn lookup_slot(&self, object_id: u32) -> Option<&WlInterface> {
        let slot = self.client_interfaces.get(object_id as usize);
        match slot {
            Some(Slot::Live(interface)) => Some(interface),
            Some(Slot::Free) => None,
            Some(Slot::Reserved) => None,
            None => None,
        }
    }
}
