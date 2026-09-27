use std::{
    env,
    io::{ErrorKind, Read, stdin},
    ops::ControlFlow,
    path::PathBuf,
    process::exit,
};

use clip_for_fun::{
    BoundInterface, DataControlDeviceEvent, DataControlOfferEvent, DataDeviceManagerExt,
    ExtDataControlManagerV1, FdWriteAndClose, WlBufferedStream, WlDataControlDevice,
    WlDataControlOffer, WlDataControlSourceEvent, WlDisplay,
    WlEvent::{self, DataControlOffer},
    WlMessageReader, WlMessageRouter, WlSeat, debug_println,
};

enum OperationMode {
    Copy { input: Vec<u8> },
    Paste,
}
const PREFERRED_MIME_TYPES_STR: [&'static str; 6] = [
    "text/plain;charset=utf-8",
    "UTF8_STRING",
    "text/plain",
    "STRING",
    "TEXT",
    "text/html",
];

const PREFERRED_MIME_TYPES: [&'static [u8]; 6] = [
    PREFERRED_MIME_TYPES_STR[0].as_bytes(),
    PREFERRED_MIME_TYPES_STR[1].as_bytes(),
    PREFERRED_MIME_TYPES_STR[2].as_bytes(),
    PREFERRED_MIME_TYPES_STR[3].as_bytes(),
    PREFERRED_MIME_TYPES_STR[4].as_bytes(),
    PREFERRED_MIME_TYPES_STR[5].as_bytes(),
];

fn main() {
    let mode = parse_operation_mode();

    let socket_path =
        PathBuf::from(env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is not set"))
            .join(env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".to_string()));
    debug_println!("Wayland socket path: {}", socket_path.display());

    let mut stream =
        WlBufferedStream::connect(&socket_path).expect("Could not connect to unix socket");
    debug_println!("Successfully connected to the Wayland socket");

    let mut display = WlDisplay::new();
    let mut router = WlMessageRouter::new();

    let (mgr_local, seat_local, local_data_device) =
        setup_wl_registry(&mut display, &mut stream, &mut router).unwrap();

    match mode {
        OperationMode::Copy { input } => {
            debug_println!(
                "Read input data: {:?}(as text: {}), length: {}",
                input,
                String::from_utf8_lossy(&input),
                input.len()
            );

            let data_source = mgr_local
                .create_data_source(&mut stream, &mut router)
                .unwrap();
            data_source.offer(&mut stream, "text/plain");
            data_source.offer(&mut stream, "text/plain;charset=utf-8");
            local_data_device.set_selection(&mut stream, data_source.local_id);

            display.sync(&mut stream, &mut router).unwrap();

            router
                .dispatch_messages(&mut stream, |wl_event| {
                    match wl_event {
                        WlEvent::DataControlSource(WlDataControlSourceEvent::Send {
                            mime_type,
                            fd,
                        }) => {
                            debug_println!(
                                "Received send event with mime_type {} and fd {:?}",
                                String::from_utf8_lossy(mime_type),
                                fd
                            );

                            // Serve to pasting client via fd it sent.
                            if fd.fd_write_and_close(&input).is_err() {
                                eprintln!("Failed to write to fd");
                            }
                            ControlFlow::Continue(())
                        }
                        WlEvent::DataControlSource(WlDataControlSourceEvent::Cancelled) => {
                            debug_println!("Received cancelled event. Exiting...");
                            ControlFlow::Break(())
                        }
                        _ => ControlFlow::Continue(()),
                    }
                })
                .unwrap();
        }
        OperationMode::Paste => {
            debug_println!("Starting Paste loop");

            let mut offers: [Option<(u32, Option<usize>)>; 2] = [None; 2];
            let mut selection: Option<u32> = None;

            router
                .dispatch_messages(&mut stream, |wl_event| match wl_event {
                    WlEvent::DataControlDevice(DataControlDeviceEvent::DataOffer { new_id }) => {
                        debug_println!(
                            "Received DataControlDevice DataOffer with new_id {}",
                            new_id
                        );

                        let mut candidate = offers.iter_mut().find(|offer| offer.is_none());
                        if let Some(slot) = candidate {
                            *slot = Some((new_id, None));
                        } else {
                            candidate = offers
                                .iter_mut()
                                .find(|offer| offer.unwrap().0 != selection.unwrap_or(0));
                            if let Some(slot) = candidate {
                                *slot = Some((new_id, None));
                            } else {
                                eprintln!(
                                    "No available slot for new DataOffer with new_id {}",
                                    new_id
                                );
                                exit(1);
                            }
                        }

                        ControlFlow::Continue(())
                    }
                    WlEvent::DataControlDevice(DataControlDeviceEvent::Selection { offer_id }) => {
                        debug_println!(
                            "Received DataControlDevice Selection with offer_id {:?}",
                            offer_id
                        );
                        selection = offer_id;
                        ControlFlow::Continue(())
                    }
                    WlEvent::DataControlDevice(DataControlDeviceEvent::Finished) => {
                        eprint!("Received DataControlDevice Finished event");
                        exit(1);
                    }
                    WlEvent::DataControlOffer {
                        id,
                        event: DataControlOfferEvent::Offer { mime },
                    } => {
                        debug_println!(
                            "Received DataControlOffer with id {} and mime_type {:?}",
                            id,
                            String::from_utf8_lossy(&mime)
                        );

                        let Some(rank) = PREFERRED_MIME_TYPES.iter().position(|p| p == &mime)
                        else {
                            return ControlFlow::Continue(());
                        };
                        let Some((_, best)) = offers
                            .iter_mut()
                            .flatten()
                            .find(|(offer_id, _)| *offer_id == id)
                        else {
                            return ControlFlow::Continue(());
                        };
                        if best.is_some_and(|b| b <= rank) {
                            return ControlFlow::Continue(());
                        }
                        debug_println!(
                            "Offer {}: best mime {:?} -> {:?}",
                            id,
                            best.map_or("None", |b| PREFERRED_MIME_TYPES_STR[b]),
                            PREFERRED_MIME_TYPES_STR[rank],
                        );
                        *best = Some(rank);

                        ControlFlow::Continue(())
                    }
                    WlEvent::SyncDone => {
                        debug_println!("Received SyncDone event from the Wayland compositor");
                        ControlFlow::Break(())
                    }
                    _ => ControlFlow::Continue(()),
                })
                .unwrap();

            if selection.is_none() {
                eprintln!("error: no selection was made");
                exit(1);
            }

            if let Some(slot) = offers
                .iter()
                .flatten()
                .find(|(id, _)| id == &selection.unwrap())
            {
                debug_println!("Selected offer: id = {}, mime index = {:?}", slot.0, slot.1);

                if slot.1.is_none() {
                    eprintln!("error: selected offer has no mime set.");
                    exit(1);
                }

                let mime = PREFERRED_MIME_TYPES_STR[slot.1.unwrap()];

                let (mut reader, writer) = std::io::pipe().unwrap();
                WlDataControlOffer::new(selection.unwrap())
                    .receive(&mut stream, mime, writer.into())
                    .unwrap(); // build: appends bytes + queues fd
                stream.write().unwrap();

                let write_result = std::io::copy(&mut reader, &mut std::io::stdout().lock());

                match write_result {
                    Ok(_) => return,
                    Err(e) if e.kind() == ErrorKind::BrokenPipe => {
                        exit(0);
                    }
                    Err(e) => {
                        eprintln!("error: failed to write to stdout: {}", e);
                        exit(1);
                    }
                }
            } else {
                eprint!("error: selected offer not found");
                exit(1);
            }
        }
    }
}

#[inline(always)]
fn setup_wl_registry(
    display: &mut WlDisplay,
    mut stream: &mut WlBufferedStream,
    mut router: &mut WlMessageRouter,
) -> Result<
    (
        BoundInterface<ExtDataControlManagerV1>,
        BoundInterface<WlSeat>,
        WlDataControlDevice,
    ),
    std::io::Error,
> {
    let mut registry = display.get_registry(&mut stream, &mut router).unwrap();
    display.sync(&mut stream, &mut router).unwrap();
    router.dispatch_messages(&mut stream, |event| match event {
        WlEvent::Registry(header, buffer) => {
            registry.add_interface(&header, &mut WlMessageReader::new(buffer));
            ControlFlow::Continue(())
        }
        WlEvent::SyncDone => ControlFlow::Break(()),
        _ => ControlFlow::Continue(()),
    })?;

    debug_println!("Got registry: {:?}", registry);

    if let Some(ext_data_control_manager) = registry.ext_data_control_manager {
        debug_println!(
            "Found ExtDataControlManagerV1({}) with id {} and version {}",
            ext_data_control_manager.interface_name.str,
            ext_data_control_manager.global_name,
            ext_data_control_manager.version
        );

        let mgr_local = registry.bind(&mut stream, &mut router, ext_data_control_manager)?;
        let seat_local = registry.bind(
            &mut stream,
            &mut router,
            registry.wl_seat.ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "wl_seat not found")
            })?,
        )?;

        let local_data_device =
            mgr_local.get_data_device(&mut stream, &mut router, seat_local.local_id)?;

        debug_println!(
            "Bound ExtDataControlManagerV1 to local id {}, and WlSeat to local id {} and got DataDevice with local id {}",
            mgr_local.local_id,
            seat_local.local_id,
            local_data_device.local_id
        );

        display.sync(&mut stream, &mut router)?;
        Ok((mgr_local, seat_local, local_data_device))
    } else {
        eprintln!("error: this compositor does not support ext_data_control_manager_v1");
        exit(1);
    }
}

#[inline(always)]
fn parse_operation_mode() -> OperationMode {
    const BOLD: &str = "\x1b[1m";
    const RED: &str = "\x1b[31m";
    const GREEN: &str = "\x1b[32m";
    const YELLOW: &str = "\x1b[33m";
    const CYAN: &str = "\x1b[36m";
    const RESET: &str = "\x1b[0m";

    let args: Vec<String> = env::args().collect();

    if args.len() == 3 && args[1] == "--" {
        debug_println!("Mode is copy. Using argument after '--' as copy content");
        return OperationMode::Copy {
            input: args[2].as_bytes().to_vec(),
        };
    }

    if args.len() > 2 {
        // print pretty usage message with console colors.

        eprintln!("{BOLD}{RED}error:{RESET} too many arguments");
        eprintln!();
        eprintln!("{BOLD}{YELLOW}Usage:{RESET}");
        eprintln!(
            "  {GREEN}{}{RESET} [{CYAN}--paste{RESET} | {CYAN}<text>{RESET}]",
            args[0]
        );
        exit(1);
    }

    if args.len() == 2 && args[1] == "--paste" {
        debug_println!("Mode is paste.");
        OperationMode::Paste
    } else if args.len() == 2 {
        debug_println!("Mode is copy. Using argument as copy content");
        OperationMode::Copy {
            input: args[1].as_bytes().to_vec(),
        }
    } else {
        debug_println!("Mode is copy. Using stdin as copy content");
        let mut input = Vec::new();
        stdin().read_to_end(&mut input).unwrap();
        OperationMode::Copy { input }
    }
}
