use crate::{
    log_debug,
    wl::{
        objects::{
            MessageHeader, WlCallbackEvents,
            wl_data_control_device::{DataControlDeviceEvent, WlDataControlDevice},
            wl_data_offer::{DataControlOfferEvent, WlDataControlOffer},
            wl_data_source::{WlDataControlSource, WlDataControlSourceEvent},
            wl_display::{DisplayEvent, WlDisplay},
        },
        wl_buffered_stream::WlBufferedStream,
    },
};
use std::{
    io::{Error, ErrorKind, Result},
    ops::ControlFlow,
};

const SERVER_ID_START: u32 = 0xff000000;

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

pub enum WlEvent<'a> {
    DataControlSource(WlDataControlSourceEvent<'a>),

    DataControlDevice(DataControlDeviceEvent),

    DataControlOffer {
        id: u32,
        event: DataControlOfferEvent<'a>,
    },

    Registry(MessageHeader, &'a [u8]),

    SyncDone,

    Ignored,
}

const MAX_REGISTERED_INTERFACES: usize = 16;

pub struct WlMessageRouter {
    client_interfaces: [Slot; MAX_REGISTERED_INTERFACES],
    server_interfaces: [Slot; MAX_REGISTERED_INTERFACES],
}

impl Default for WlMessageRouter {
    fn default() -> Self {
        Self::new()
    }
}

pub struct WlPendingId<'a> {
    id: u32,
    slot: &'a mut Slot,
}

impl<'a> WlPendingId<'a> {
    pub(crate) fn new(id: u32, slot: &'a mut Slot) -> Self {
        Self { id, slot }
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn commit(self) -> u32 {
        let id = self.id;
        std::mem::forget(self);
        id
    }
}

impl<'a> Drop for WlPendingId<'a> {
    fn drop(&mut self) {
        *self.slot = Slot::Free;
    }
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

    pub fn register_client(&mut self, interface: WlInterface) -> Result<WlPendingId<'_>> {
        if let Some(free_index) = self.find_free_client() {
            let slot = &mut self.client_interfaces[free_index];
            *slot = Slot::Live(interface);
            log_debug!(
                "Registered interface {:?} at client slot {}",
                interface,
                free_index
            );
            Ok(WlPendingId::new(free_index as u32, slot))
        } else {
            Err(Error::other("No free client slot available"))
        }
    }

    pub fn register_server(&mut self, interface: WlInterface, id: u32) -> Result<u32> {
        if let Some(slot) = self.server_interfaces.get_mut(
            id.checked_sub(SERVER_ID_START)
                .ok_or_else(|| Error::other(format!("Invalid server slot id {}", id)))?
                as usize,
        ) {
            *slot = Slot::Live(interface);
            log_debug!("Registered interface {:?} at server slot {}", interface, id);
            Ok(id)
        } else {
            Err(Error::other(format!(
                "Invalid server slot id {}(local array id: {})",
                id,
                (id - SERVER_ID_START)
            )))
        }
    }

    fn next_event<'a>(
        &mut self,
        stream: &'a mut WlBufferedStream,
    ) -> std::io::Result<Option<WlEvent<'a>>> {
        let Some((header, buffer, fds)) = stream.read_next_message()? else {
            return Ok(None);
        };
        {
            // log_debug!("Received message with header: {:?}", header);
            let interface = match self.lookup_slot(header.object_id) {
                Some(interface) => interface,
                // compositor-created object we don't track (e.g. an offer)
                None if header.object_id >= 0xff00_0000 => return Ok(Some(WlEvent::Ignored)),
                None => return Err(Error::other("No slot found")),
            };

            match interface {
                WlInterface::Callback => {
                    if header.opcode == WlCallbackEvents::Done as u16 {
                        return Ok(Some(WlEvent::SyncDone));
                    }
                    Err(Error::other("Unknown opcode"))
                }
                WlInterface::Display => {
                    let Some(display_event) = WlDisplay::parse_message(header.opcode, buffer)
                    else {
                        return Ok(Some(WlEvent::Ignored));
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
                            log_debug!("Deleting WL client object id: {}", id);
                            if let Some(slot) = self.client_interfaces.get_mut(id as usize) {
                                *slot = Slot::Free;
                            }
                            Ok(Some(WlEvent::Ignored))
                        }
                    }
                }
                WlInterface::Seat => Ok(Some(WlEvent::Ignored)),
                WlInterface::ExtDataControlManager => Ok(Some(WlEvent::Ignored)),
                WlInterface::ZwlrDataControlManager => Ok(Some(WlEvent::Ignored)),
                WlInterface::DataDeviceManager => Ok(Some(WlEvent::Ignored)),
                WlInterface::DataControlDevice => {
                    let data_control_device_event =
                        WlDataControlDevice::parse_message(header.opcode, buffer, fds)?;
                    if let DataControlDeviceEvent::DataOffer { new_id } = data_control_device_event
                    {
                        self.register_server(WlInterface::DataControlOffer, new_id)?;
                    }
                    Ok(Some(WlEvent::DataControlDevice(data_control_device_event)))
                }
                WlInterface::DataControlSource => Ok(Some(WlEvent::DataControlSource(
                    WlDataControlSource::parse_message(header.opcode, buffer, fds)?,
                ))),
                WlInterface::DataControlOffer => {
                    let data_control_offer_event =
                        WlDataControlOffer::parse_message(header.opcode, buffer, fds)?;
                    Ok(Some(WlEvent::DataControlOffer {
                        id: header.object_id,
                        event: data_control_offer_event,
                    }))
                }
                WlInterface::Registry => Ok(Some(WlEvent::Registry(header, buffer))),
            }
        }
    }

    pub fn free_server(&mut self, object_id: u32) -> Result<()> {
        if let Some(slot) = self.server_interfaces.get_mut(
            object_id
                .checked_sub(SERVER_ID_START)
                .ok_or_else(|| Error::other(format!("Invalid server slot id {}", object_id)))?
                as usize,
        ) {
            *slot = Slot::Free;
        }
        Ok(())
    }

    pub fn free_client(&mut self, object_id: u32) -> Option<()> {
        if let Some(slot) = self.client_interfaces.get_mut(object_id as usize) {
            *slot = Slot::Free;
            return Some(());
        }
        None
    }

    pub fn dispatch_messages<F: FnMut(WlEvent<'_>) -> ControlFlow<Result<()>>>(
        &mut self,
        stream: &mut WlBufferedStream,
        mut fnhandler: F,
    ) -> std::io::Result<()> {
        log_debug!("Starting message dispatch loop");
        while let Some(event) = self.next_event(stream)? {
            if let ControlFlow::Break(result) = fnhandler(event) {
                return result;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        FdWriteAndClose,
        wl::{
            objects::{
                wl_data_control_device::WlDataControlDeviceEvents,
                wl_data_offer::{WlDataControlOfferEvents, WlDataControlOfferOps},
                wl_display::DisplayEvents,
            },
            wl_message_reader::WlMessageReader,
        },
    };
    use std::{
        io::{Read, Write},
        os::unix::net::UnixStream,
        time::Duration,
    };

    fn msg(object_id: u32, opcode: u16, body: &[u8]) -> Vec<u8> {
        let mut m = Vec::with_capacity(8 + body.len());
        m.extend_from_slice(&object_id.to_ne_bytes());
        m.extend_from_slice(&(((8 + body.len()) as u32) << 16 | u32::from(opcode)).to_ne_bytes());
        m.extend_from_slice(body);
        m
    }

    fn wl_str(s: &str) -> Vec<u8> {
        let mut out = ((s.len() + 1) as u32).to_ne_bytes().to_vec();
        out.extend_from_slice(s.as_bytes());
        out.push(0);
        out.resize(out.len().next_multiple_of(4), 0);
        out
    }

    // The peer end plays the compositor.
    fn setup() -> (WlMessageRouter, WlBufferedStream, UnixStream) {
        let (a, peer) = UnixStream::pair().unwrap();
        (
            WlMessageRouter::new(),
            WlBufferedStream::new(a.into()),
            peer,
        )
    }

    #[test]
    fn ids_are_dense_and_reused_after_delete_id() {
        let (mut router, mut stream, mut peer) = setup();
        assert_eq!(
            router
                .register_client(WlInterface::Registry)
                .unwrap()
                .commit(),
            2
        );
        assert_eq!(
            router
                .register_client(WlInterface::Callback)
                .unwrap()
                .commit(),
            3
        );
        assert_eq!(
            router.register_client(WlInterface::Seat).unwrap().commit(),
            4
        );

        peer.write_all(&msg(
            WlDisplay::TYPE_ID,
            DisplayEvents::DeleteId.into(),
            &3u32.to_ne_bytes(),
        ))
        .unwrap();
        assert!(matches!(
            router.next_event(&mut stream).unwrap(),
            Some(WlEvent::Ignored)
        ));
        assert_eq!(
            router
                .register_client(WlInterface::DataControlSource)
                .unwrap()
                .commit(),
            3
        );
    }

    #[test]
    fn a_dropped_pending_id_frees_its_slot() {
        let mut router = WlMessageRouter::new();
        let pending = router.register_client(WlInterface::Registry).unwrap();
        assert_eq!(pending.id(), 2);
        drop(pending);
        assert_eq!(router.lookup_slot(2), None);
        assert_eq!(
            router.register_client(WlInterface::Seat).unwrap().commit(),
            2
        );
        assert_eq!(router.lookup_slot(2), Some(WlInterface::Seat));
    }

    #[test]
    fn data_offer_registers_the_offer_and_its_events_carry_its_id() {
        let (mut router, mut stream, mut peer) = setup();
        let device = router
            .register_client(WlInterface::DataControlDevice)
            .unwrap()
            .commit();
        let offer: u32 = 0xff00_0000;

        let mut bytes = msg(
            device,
            WlDataControlDeviceEvents::DataOffer.into(),
            &offer.to_ne_bytes(),
        );
        bytes.extend(msg(
            offer,
            WlDataControlOfferEvents::Offer.into(),
            &wl_str("text/plain"),
        ));
        bytes.extend(msg(
            device,
            WlDataControlDeviceEvents::Selection.into(),
            &offer.to_ne_bytes(),
        ));
        bytes.extend(msg(
            device,
            WlDataControlDeviceEvents::PrimarySelection.into(),
            &0u32.to_ne_bytes(),
        ));
        peer.write_all(&bytes).unwrap();

        assert!(matches!(
            router.next_event(&mut stream).unwrap(),
            Some(WlEvent::DataControlDevice(DataControlDeviceEvent::DataOffer { new_id })) if new_id == offer
        ));
        assert!(matches!(
            router.next_event(&mut stream).unwrap(),
            Some(WlEvent::DataControlOffer { id, event: DataControlOfferEvent::Offer { mime } })
                if id == offer && mime == b"text/plain"
        ));
        assert!(matches!(
            router.next_event(&mut stream).unwrap(),
            Some(WlEvent::DataControlDevice(DataControlDeviceEvent::Selection { offer_id: Some(id) })) if id == offer
        ));
        assert!(matches!(
            router.next_event(&mut stream).unwrap(),
            Some(WlEvent::DataControlDevice(
                DataControlDeviceEvent::PrimarySelection { offer_id: None }
            ))
        ));
    }

    #[test]
    fn unknown_server_object_is_ignored_but_unknown_client_object_is_an_error() {
        let (mut router, mut stream, mut peer) = setup();
        peer.write_all(&msg(0xff00_0005, 0, &wl_str("text/plain")))
            .unwrap();
        assert!(matches!(
            router.next_event(&mut stream).unwrap(),
            Some(WlEvent::Ignored)
        ));

        peer.write_all(&msg(9, 0, &[])).unwrap();
        assert!(router.next_event(&mut stream).is_err());
    }

    #[test]
    fn malformed_or_unknown_events_are_errors() {
        let (mut router, mut stream, mut peer) = setup();
        let device = router
            .register_client(WlInterface::DataControlDevice)
            .unwrap()
            .commit();

        peer.write_all(&msg(device, 9, &[])).unwrap();
        assert!(router.next_event(&mut stream).is_err(), "unknown opcode");

        peer.write_all(&msg(
            device,
            WlDataControlDeviceEvents::Selection.into(),
            &[],
        ))
        .unwrap();
        assert!(router.next_event(&mut stream).is_err(), "missing argument");

        peer.write_all(&msg(
            device,
            WlDataControlDeviceEvents::DataOffer.into(),
            &5u32.to_ne_bytes(),
        ))
        .unwrap();
        assert!(
            router.next_event(&mut stream).is_err(),
            "new_id outside server range"
        );
    }

    #[test]
    fn callback_done_is_sync_done_and_display_error_is_an_error() {
        let (mut router, mut stream, mut peer) = setup();
        let callback = router
            .register_client(WlInterface::Callback)
            .unwrap()
            .commit();
        peer.write_all(&msg(
            callback,
            WlCallbackEvents::Done as u16,
            &7u32.to_ne_bytes(),
        ))
        .unwrap();
        assert!(matches!(
            router.next_event(&mut stream).unwrap(),
            Some(WlEvent::SyncDone)
        ));

        let mut body = 2u32.to_ne_bytes().to_vec();
        body.extend(1u32.to_ne_bytes());
        body.extend(wl_str("invalid arguments"));
        peer.write_all(&msg(WlDisplay::TYPE_ID, DisplayEvents::Error.into(), &body))
            .unwrap();
        let Err(err) = router.next_event(&mut stream) else {
            panic!("display error was not reported");
        };
        assert!(err.to_string().contains("invalid arguments"));
    }

    #[test]
    fn dispatch_stops_on_break_and_reports_unexpected_eof() {
        let (mut router, mut stream, mut peer) = setup();
        let callback = router
            .register_client(WlInterface::Callback)
            .unwrap()
            .commit();
        peer.write_all(&msg(
            callback,
            WlCallbackEvents::Done as u16,
            &0u32.to_ne_bytes(),
        ))
        .unwrap();
        drop(peer);

        router
            .dispatch_messages(&mut stream, |event| {
                if matches!(event, WlEvent::SyncDone) {
                    ControlFlow::Break(Ok(()))
                } else {
                    ControlFlow::Continue(())
                }
            })
            .unwrap();
        let err = router
            .dispatch_messages(&mut stream, |_| ControlFlow::Continue(()))
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::UnexpectedEof);
    }

    #[test]
    fn receive_sends_mime_with_fd_and_destroy_frees_the_offer() {
        let (a, b) = UnixStream::pair().unwrap();
        let mut client = WlBufferedStream::new(a.into());
        let mut compositor = WlBufferedStream::new(b.into());
        let mut router = WlMessageRouter::new();
        let offer_id: u32 = 0xff00_0000;
        router
            .register_server(WlInterface::DataControlOffer, offer_id)
            .unwrap();

        let (mut payload_rx, payload_tx) = UnixStream::pair().unwrap();
        let offer = WlDataControlOffer::new(offer_id);
        offer
            .receive(client.get_writer(), "text/plain", payload_tx.into())
            .unwrap();
        offer.destroy(client.get_writer(), &mut router).unwrap();
        client.write().unwrap();

        let (header, body, fds) = compositor.read_next_message().unwrap().unwrap();
        assert_eq!(
            (header.object_id, header.opcode),
            (offer_id, u16::from(WlDataControlOfferOps::Receive))
        );
        assert_eq!(WlMessageReader::new(body).str(), Some(&b"text/plain"[..]));
        fds.pop_last_in_fd()
            .unwrap()
            .fd_write_and_close(b"hello")
            .unwrap();

        let (header, ..) = compositor.read_next_message().unwrap().unwrap();
        assert_eq!(
            (header.object_id, header.opcode, header.size),
            (offer_id, u16::from(WlDataControlOfferOps::Destroy), 8)
        );
        assert_eq!(router.lookup_slot(offer_id), None);

        // Times out instead of hanging if the client kept its copy of the write end.
        payload_rx
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut got = Vec::new();
        payload_rx.read_to_end(&mut got).unwrap();
        assert_eq!(got, b"hello");
    }

    #[test]
    fn paste_burst_is_routed_per_offer_and_messages_after_done_are_kept() {
        let (mut router, mut stream, mut peer) = setup();
        let seat = router.register_client(WlInterface::Seat).unwrap().commit();
        let device = router
            .register_client(WlInterface::DataControlDevice)
            .unwrap()
            .commit();
        let callback = router
            .register_client(WlInterface::Callback)
            .unwrap()
            .commit();
        let (clipboard, primary) = (0xff00_0000u32, 0xff00_0001u32);

        // What a compositor sends right after bind + get_data_device + sync, in one write.
        let mut burst = msg(seat, 0, &3u32.to_ne_bytes());
        burst.extend(msg(
            device,
            WlDataControlDeviceEvents::DataOffer.into(),
            &clipboard.to_ne_bytes(),
        ));
        burst.extend(msg(
            clipboard,
            WlDataControlOfferEvents::Offer.into(),
            &wl_str("text/html"),
        ));
        burst.extend(msg(
            clipboard,
            WlDataControlOfferEvents::Offer.into(),
            &wl_str("text/plain;charset=utf-8"),
        ));
        burst.extend(msg(
            device,
            WlDataControlDeviceEvents::Selection.into(),
            &clipboard.to_ne_bytes(),
        ));
        burst.extend(msg(
            device,
            WlDataControlDeviceEvents::DataOffer.into(),
            &primary.to_ne_bytes(),
        ));
        burst.extend(msg(
            primary,
            WlDataControlOfferEvents::Offer.into(),
            &wl_str("text/plain"),
        ));
        burst.extend(msg(
            device,
            WlDataControlDeviceEvents::PrimarySelection.into(),
            &primary.to_ne_bytes(),
        ));
        burst.extend(msg(
            callback,
            WlCallbackEvents::Done as u16,
            &0u32.to_ne_bytes(),
        ));
        burst.extend(msg(
            WlDisplay::TYPE_ID,
            DisplayEvents::DeleteId.into(),
            &callback.to_ne_bytes(),
        ));
        peer.write_all(&burst).unwrap();

        let mut mimes = Vec::new();
        let mut selection = None;
        router
            .dispatch_messages(&mut stream, |event| {
                match event {
                    WlEvent::DataControlOffer {
                        id,
                        event: DataControlOfferEvent::Offer { mime },
                    } => mimes.push((id, mime.to_vec())),
                    WlEvent::DataControlDevice(DataControlDeviceEvent::Selection { offer_id }) => {
                        selection = offer_id
                    }
                    WlEvent::SyncDone => return ControlFlow::Break(Ok(())),
                    _ => {}
                }
                ControlFlow::Continue(())
            })
            .unwrap();

        assert_eq!(selection, Some(clipboard));
        assert_eq!(
            mimes,
            [
                (clipboard, b"text/html".to_vec()),
                (clipboard, b"text/plain;charset=utf-8".to_vec()),
                (primary, b"text/plain".to_vec()),
            ]
        );

        // delete_id arrived in the same read, after done: it must not be dropped.
        assert!(matches!(
            router.next_event(&mut stream).unwrap(),
            Some(WlEvent::Ignored)
        ));
        assert_eq!(
            router
                .register_client(WlInterface::Callback)
                .unwrap()
                .commit(),
            callback
        );
    }
}
