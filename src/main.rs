use std::{
    env,
    ffi::OsString,
    io::{self, ErrorKind, Read, stdin},
    ops::ControlFlow,
    os::unix::ffi::OsStringExt,
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
    let mut args: Vec<OsString> = env::args_os().collect();
    let mode = parse_operation_mode(&mut args)?;

    let runtime_dir = env::var_os("XDG_RUNTIME_DIR")
        .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "XDG_RUNTIME_DIR is not set"))?;

    let socket_path = PathBuf::from(&runtime_dir)
        .join(env::var_os("WAYLAND_DISPLAY").unwrap_or_else(|| OsString::from("wayland-0")));
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
            log_debug!("Read {} bytes of input", input.len());

            let data_source = mgr_local.create_data_source(&mut stream, &mut router)?;
            for mime in OFFERED_TXT_MIME_TYPES {
                data_source.offer(&mut stream, mime)?;
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

fn parse_operation_mode(args: &mut Vec<OsString>) -> Result<OperationMode, std::io::Error> {
    match args.len() {
        3 if args[1] == "--" => {
            log_debug!("Mode is copy. Using argument after '--' as copy content");
            Ok(OperationMode::Copy {
                input: args.swap_remove(2).into_vec(),
            })
        }
        2 if args[1] == "--paste" => {
            log_debug!("Mode is paste.");
            Ok(OperationMode::Paste)
        }
        2 => {
            log_debug!("Mode is copy. Using argument as copy content");

            Ok(OperationMode::Copy {
                input: args.swap_remove(1).into_vec(),
            })
        }
        1 => {
            log_debug!("Mode is copy. Using stdin as copy content");
            let mut input = Vec::new();
            stdin().read_to_end(&mut input)?;
            Ok(OperationMode::Copy { input })
        }
        _ => {
            let Colors {
                bold,
                yellow,
                green,
                cyan,
                reset,
                ..
            } = LOGGER.colors;
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "too many arguments\n\
                 \n\
                 {bold}{yellow}Usage:{reset}\n  {green}{}{reset} [{cyan}--paste{reset} | {cyan}<text>{reset}]",
                    args[0].display()
                ),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{ffi::OsStr, os::unix::ffi::OsStrExt};

    fn args(list: &[&[u8]]) -> Vec<OsString> {
        list.iter()
            .map(|a| OsStr::from_bytes(a).to_os_string())
            .collect()
    }

    #[test]
    fn non_utf8_argument_is_copied_byte_for_byte() {
        let mode = parse_operation_mode(&mut args(&[b"clip", b"\xff\xfe"]));
        assert!(matches!(mode, Ok(OperationMode::Copy { input }) if input == b"\xff\xfe"));
    }

    #[test]
    fn paste_flag_selects_paste() {
        let mode = parse_operation_mode(&mut args(&[b"clip", b"--paste"]));
        assert!(matches!(mode, Ok(OperationMode::Paste)));
    }

    #[test]
    fn double_dash_copies_text_that_looks_like_a_flag() {
        let mode = parse_operation_mode(&mut args(&[b"clip", b"--", b"--paste"]));
        assert!(matches!(mode, Ok(OperationMode::Copy { input }) if input == b"--paste"));
    }

    #[test]
    fn too_many_arguments_is_an_error() {
        for list in [
            &[&b"clip"[..], b"a", b"b"][..],
            &[b"clip", b"a", b"b", b"c"],
        ] {
            let mode = parse_operation_mode(&mut args(list));
            assert!(matches!(mode, Err(e) if e.kind() == ErrorKind::InvalidInput));
        }
    }
}
