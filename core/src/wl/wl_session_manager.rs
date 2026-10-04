use std::ops::ControlFlow;

use crate::{
    BoundInterface, DataDeviceManagerExt, ExtDataControlManagerV1, WlBufferedStream,
    WlDataControlDevice, WlDisplay, WlEvent, WlMessageReader, WlMessageRouter, log_debug,
    wl::{objects::wl_data_source::WlDataControlSource, wl_message_writer::WlMessageWriter},
};

pub struct WlSessionManager {
    display: WlDisplay,
    stream: WlBufferedStream,
    router: WlMessageRouter,
    local_data_device: WlDataControlDevice,
    ext_data_control_manager: BoundInterface<ExtDataControlManagerV1>,
}

impl WlSessionManager {
    pub fn initialize(mut stream: WlBufferedStream) -> Result<WlSessionManager, std::io::Error> {
        let mut display = WlDisplay::new();
        let mut router = WlMessageRouter::new();
        let mut registry = display.get_registry(stream.get_writer(), &mut router)?;
        display.schedule_sync(stream.get_writer(), &mut router)?;
        stream.write()?;
        router.dispatch_messages(&mut stream, |event| match event {
            WlEvent::Registry(header, buffer) => {
                registry.add_interface(&header, &mut WlMessageReader::new(buffer));
                ControlFlow::Continue(())
            }
            WlEvent::SyncDone => ControlFlow::Break(Ok(())),
            _ => ControlFlow::Continue(()),
        })?;

        log_debug!("Got registry: {:?}", registry);

        if let Some(ext_data_control_manager) = registry.ext_data_control_manager {
            log_debug!(
                "Found ExtDataControlManagerV1({}) with id {} and version {}",
                ext_data_control_manager.interface_name.str,
                ext_data_control_manager.global_name,
                ext_data_control_manager.version
            );

            let mgr_local =
                registry.bind(stream.get_writer(), &mut router, ext_data_control_manager)?;
            let seat_local = registry.bind(
                stream.get_writer(),
                &mut router,
                registry.wl_seat.ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::NotFound, "wl_seat not found")
                })?,
            )?;

            let local_data_device =
                mgr_local.get_data_device(stream.get_writer(), &mut router, seat_local.local_id)?;

            log_debug!(
                "Bound ExtDataControlManagerV1 to local id {}, and WlSeat to local id {} and got DataDevice with local id {}",
                mgr_local.local_id,
                seat_local.local_id,
                local_data_device.local_id
            );

            Ok(WlSessionManager {
                display,
                stream,
                router,
                ext_data_control_manager: mgr_local,
                local_data_device,
            })
        } else {
            Err(std::io::Error::other(
                "this compositor does not support ext_data_control_manager_v1",
            ))
        }
    }

    pub fn create_data_source(&mut self) -> Result<WlDataControlSource, std::io::Error> {
        self.ext_data_control_manager
            .create_data_source(self.stream.get_writer(), &mut self.router)
    }

    pub fn set_selection(&mut self, source_id: u32) -> std::io::Result<()> {
        self.local_data_device
            .set_selection(self.stream.get_writer(), source_id)
    }

    pub fn sync(&mut self) -> Result<(), std::io::Error> {
        self.display
            .schedule_sync(self.stream.get_writer(), &mut self.router)?;
        self.stream.write()
    }

    pub fn dispatch_messages<F>(&mut self, f: F) -> Result<(), std::io::Error>
    where
        F: FnMut(WlEvent) -> ControlFlow<Result<(), std::io::Error>>,
    {
        self.router.dispatch_messages(&mut self.stream, f)
    }

    pub fn get_message_writer(&mut self) -> WlMessageWriter<'_> {
        self.stream.get_writer()
    }

    pub fn send_messages(&mut self) -> Result<(), std::io::Error> {
        self.stream.write()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wl::objects::{
        WlCallbackEvents,
        wl_data_managers::ExtDataControlManagerOps,
        wl_display::DisplayOps,
        wl_registry::{RegistryEvents, RegistryOps},
    };
    use std::{
        io::{ErrorKind, Write},
        os::unix::net::UnixStream,
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

    // First ids the client hands out: 0 is reserved, 1 is wl_display.
    const REGISTRY_ID: u32 = 2;
    const CALLBACK_ID: u32 = 3;

    // Queues the compositor's replies to get_registry + sync before the client reads them.
    fn compositor_with_globals(globals: &[(u32, &str, u32)]) -> (WlBufferedStream, UnixStream) {
        let (client, mut compositor) = UnixStream::pair().unwrap();
        let mut events = Vec::new();
        for &(name, interface, version) in globals {
            let mut body = name.to_ne_bytes().to_vec();
            body.extend(wl_str(interface));
            body.extend(version.to_ne_bytes());
            events.extend(msg(REGISTRY_ID, RegistryEvents::Global as u16, &body));
        }
        events.extend(msg(
            CALLBACK_ID,
            WlCallbackEvents::Done as u16,
            &0u32.to_ne_bytes(),
        ));
        compositor.write_all(&events).unwrap();
        (WlBufferedStream::new(client.into()), compositor)
    }

    #[test]
    fn initialize_binds_the_manager_and_the_seat_and_creates_a_device() {
        let (stream, compositor) = compositor_with_globals(&[
            (1, "wl_seat", 9),
            (7, "wl_output", 4),
            (54, "ext_data_control_manager_v1", 1),
        ]);
        let mut session = WlSessionManager::initialize(stream).unwrap();
        session.send_messages().unwrap();

        let mut compositor = WlBufferedStream::new(compositor.into());
        let mut next = || {
            let (header, body, _) = compositor.read_next_message().unwrap().unwrap();
            (header.object_id, header.opcode, body.to_vec())
        };

        let (id, op, body) = next();
        assert_eq!(
            (id, op),
            (WlDisplay::TYPE_ID, DisplayOps::GetRegistry.into())
        );
        assert_eq!(WlMessageReader::new(&body).u32(), Some(REGISTRY_ID));

        let (id, op, body) = next();
        assert_eq!((id, op), (WlDisplay::TYPE_ID, DisplayOps::Sync.into()));
        assert_eq!(WlMessageReader::new(&body).u32(), Some(CALLBACK_ID));

        // Server versions 1 and 9 are clamped to what is implemented (1 for both).
        for (name, interface, new_id) in [
            (54, &b"ext_data_control_manager_v1"[..], 4),
            (1, b"wl_seat", 5),
        ] {
            let (id, op, body) = next();
            assert_eq!((id, op), (REGISTRY_ID, RegistryOps::Bind.into()));
            let mut r = WlMessageReader::new(&body);
            assert_eq!(
                (r.u32(), r.str(), r.u32(), r.u32()),
                (Some(name), Some(interface), Some(1), Some(new_id))
            );
        }

        let (id, op, body) = next();
        assert_eq!(
            (id, op),
            (4, ExtDataControlManagerOps::GetDataDevice.into())
        );
        let mut r = WlMessageReader::new(&body);
        assert_eq!(
            (r.u32(), r.u32()),
            (Some(6), Some(5)),
            "new device id, seat id"
        );
    }

    #[test]
    fn initialize_fails_without_ext_data_control() {
        let (stream, _compositor) = compositor_with_globals(&[(1, "wl_seat", 9)]);
        let Err(err) = WlSessionManager::initialize(stream) else {
            panic!("initialized without ext_data_control_manager_v1");
        };
        assert!(err.to_string().contains("ext_data_control_manager_v1"));
    }

    #[test]
    fn initialize_fails_without_a_seat() {
        let (stream, _compositor) =
            compositor_with_globals(&[(54, "ext_data_control_manager_v1", 1)]);
        let Err(err) = WlSessionManager::initialize(stream) else {
            panic!("initialized without wl_seat");
        };
        assert_eq!(err.kind(), ErrorKind::NotFound);
    }
}
