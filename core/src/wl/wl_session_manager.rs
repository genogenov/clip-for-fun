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
                display: display,
                stream: stream,
                router: router,
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
