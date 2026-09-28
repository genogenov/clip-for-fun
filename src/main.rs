use std::{
    env,
    io::{self, ErrorKind, Read, stdin},
    ops::ControlFlow,
    path::PathBuf,
    process::ExitCode,
};

use clip_for_fun::{
    BoundInterface, Colors, DataDeviceManagerExt, ExtDataControlManagerV1, FdWriteAndClose, LOGGER,
    OFFERED_TXT_MIME_TYPES, WlBufferedStream, WlDataControlDevice, WlDataControlOffer,
    WlDataControlSourceEvent, WlDisplay,
    WlEvent::{self},
    WlMessageReader, WlMessageRouter, WlOfferTracker, WlSeat, log_debug, log_error,
};

enum OperationMode {
    Copy { input: Vec<u8> },
    Paste,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            log_error!("{}", err);
            ExitCode::FAILURE
        }
    }
}

fn run() -> io::Result<()> {
    let mode = parse_operation_mode()?;

    let runtime_dir = env::var("XDG_RUNTIME_DIR")
        .map_err(|_| io::Error::new(ErrorKind::NotFound, "XDG_RUNTIME_DIR is not set"))?;

    let socket_path = PathBuf::from(&runtime_dir)
        .join(env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".to_string()));
    log_debug!("Wayland socket path: {}", socket_path.display());

    let mut stream = WlBufferedStream::connect(&socket_path).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("cannot connect to {}: {e}", socket_path.display()),
        )
    })?;
    log_debug!("Successfully connected to the Wayland socket");

    let mut display = WlDisplay::new();
    let mut router = WlMessageRouter::new();

    let (mgr_local, _seat_local, local_data_device) =
        setup_wl_registry(&mut display, &mut stream, &mut router)?;

    match mode {
        OperationMode::Copy { input } => {
            log_debug!(
                "Read input data: {:?}(as text: {}), length: {}",
                input,
                String::from_utf8_lossy(&input),
                input.len()
            );

            let data_source = mgr_local.create_data_source(&mut stream, &mut router)?;
            for mime in OFFERED_TXT_MIME_TYPES {
                data_source.offer(&mut stream, mime);
            }
            local_data_device.set_selection(&mut stream, data_source.local_id);

            display.sync(&mut stream, &mut router)?;

            router.dispatch_messages(&mut stream, |wl_event| {
                match wl_event {
                    WlEvent::DataControlSource(WlDataControlSourceEvent::Send {
                        mime_type: _,
                        fd,
                    }) => {
                        log_debug!("Received send event with fd {:?}", fd);

                        // Serve to pasting client via fd it sent.
                        if fd.fd_write_and_close(&input).is_err() {
                            log_error!("Failed to write to fd");
                        }
                        ControlFlow::Continue(())
                    }
                    WlEvent::DataControlSource(WlDataControlSourceEvent::Cancelled) => {
                        log_debug!("Received cancelled event. Exiting...");
                        ControlFlow::Break(Ok(()))
                    }
                    _ => ControlFlow::Continue(()),
                }
            })
        }
        OperationMode::Paste => {
            log_debug!("Starting Paste loop");

            let mut tracker = WlOfferTracker::new();

            router.dispatch_messages(&mut stream, |wl_event| tracker.handle_event(&wl_event))?;

            let Some(selection_id) = tracker.selection_id else {
                return Err(std::io::Error::other("nothing found to paste"));
            };

            if let Some(slot) = tracker.get_selected_slot() {
                log_debug!(
                    "Selected offer: id = {}, mime = {:?}",
                    slot.id,
                    slot.preferred_mime()
                );

                let Some(preferred_mime) = slot.preferred_mime() else {
                    return Err(std::io::Error::other("clipboard has no text content."));
                };

                let (mut reader, writer) = std::io::pipe()?;
                WlDataControlOffer::new(selection_id).receive(
                    &mut stream,
                    preferred_mime,
                    writer.into(),
                )?;
                stream.write()?;

                match std::io::copy(&mut reader, &mut std::io::stdout().lock()) {
                    Err(e) if e.kind() == ErrorKind::BrokenPipe => Ok(()),
                    other => other.map(drop),
                }
            } else {
                Err(std::io::Error::other("selected offer not found"))
            }
        }
    }
}

fn setup_wl_registry(
    display: &mut WlDisplay,
    stream: &mut WlBufferedStream,
    router: &mut WlMessageRouter,
) -> Result<
    (
        BoundInterface<ExtDataControlManagerV1>,
        BoundInterface<WlSeat>,
        WlDataControlDevice,
    ),
    std::io::Error,
> {
    let mut registry = display.get_registry(stream, router)?;
    display.sync(stream, router)?;
    router.dispatch_messages(stream, |event| match event {
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

        let mgr_local = registry.bind(stream, router, ext_data_control_manager)?;
        let seat_local = registry.bind(
            stream,
            router,
            registry.wl_seat.ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "wl_seat not found")
            })?,
        )?;

        let local_data_device = mgr_local.get_data_device(stream, router, seat_local.local_id)?;

        log_debug!(
            "Bound ExtDataControlManagerV1 to local id {}, and WlSeat to local id {} and got DataDevice with local id {}",
            mgr_local.local_id,
            seat_local.local_id,
            local_data_device.local_id
        );

        display.sync(stream, router)?;
        Ok((mgr_local, seat_local, local_data_device))
    } else {
        Err(std::io::Error::other(
            "this compositor does not support ext_data_control_manager_v1",
        ))
    }
}

fn parse_operation_mode() -> Result<OperationMode, std::io::Error> {
    let args: Vec<String> = env::args().collect();

    if args.len() == 3 && args[1] == "--" {
        log_debug!("Mode is copy. Using argument after '--' as copy content");
        return Ok(OperationMode::Copy {
            input: args[2].as_bytes().to_vec(),
        });
    }

    if args.len() > 2 {
        let Colors {
            bold,
            yellow,
            green,
            cyan,
            reset,
            ..
        } = LOGGER.colors;
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "too many arguments\n\
                 \n\
                 {bold}{yellow}Usage:{reset}\n  {green}{}{reset} [{cyan}--paste{reset} | {cyan}<text>{reset}]",
                args[0]
            ),
        ));
    }

    if args.len() == 2 && args[1] == "--paste" {
        log_debug!("Mode is paste.");
        Ok(OperationMode::Paste)
    } else if args.len() == 2 {
        log_debug!("Mode is copy. Using argument as copy content");
        Ok(OperationMode::Copy {
            input: args[1].as_bytes().to_vec(),
        })
    } else {
        log_debug!("Mode is copy. Using stdin as copy content");
        let mut input = Vec::new();
        stdin().read_to_end(&mut input)?;
        Ok(OperationMode::Copy { input })
    }
}
