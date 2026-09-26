mod unix_fd_stream;
mod wl;

use std::{
    env,
    io::{Read, stdin},
    path::PathBuf,
    process::exit,
};

use crate::wl::{
    debug_println,
    objects::{
        wl_data_managers::DataDeviceManagerExt, wl_data_source::WlDataControlSourceEvent,
        wl_display::WlDisplay,
    },
    wl_buffered_stream::WLBufferedStream,
};

fn main() {
    let socket_path =
        PathBuf::from(env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is not set"))
            .join(env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".to_string()));
    debug_println!("Wayland socket path: {}", socket_path.display());

    let mut stream =
        WLBufferedStream::connect(&socket_path).expect("Could not connect to unix socket");
    debug_println!("Successfully connected to the Wayland socket");

    let mut display = WlDisplay::new();

    // Data to put on the clipboard: first argument if given, otherwise all of stdin.
    let in_str: Vec<u8> = match env::args().nth(1) {
        Some(first_arg) => {
            debug_println!("Using argument: {}", first_arg);
            first_arg.into_bytes()
        }
        None => {
            debug_println!("Reading entire input from stdin");
            let mut in_vec = Vec::new();
            stdin().read_to_end(&mut in_vec).unwrap();
            in_vec
        }
    };
    debug_println!("Input data length : {}", in_str.len());

    let mut registry = display.get_registry(&mut stream).unwrap();
    display.roundtrip_sync(&mut stream).unwrap();
    display
        .dispatch_messages(&mut stream, |header, buffer, _, offset| {
            registry.add_interface(header, buffer, offset);
        })
        .unwrap();

    debug_println!("Got registry: {:?}", registry);

    if let Some(ext_data_control_manager) = registry.ext_data_control_manager {
        debug_println!(
            "Found ExtDataControlManagerV1({}) with id {} and version {}",
            ext_data_control_manager.interface_name.str,
            ext_data_control_manager.global_name,
            ext_data_control_manager.version
        );

        let mgr_local = registry
            .bind(&mut stream, ext_data_control_manager)
            .unwrap();
        let seat_local = registry
            .bind(&mut stream, registry.wl_seat.unwrap())
            .unwrap();

        let local_data_device = mgr_local.get_data_device(&mut stream, seat_local.local_id);
        let mut data_source = mgr_local.create_data_source(&mut stream);
        data_source.offer(&mut stream, "text/plain");
        data_source.offer(&mut stream, "text/plain;charset=utf-8");

        local_data_device.set_selection(&mut stream, data_source.local_id);

        debug_println!(
            "Bound ExtDataControlManagerV1 to local id {}, and WlSeat to local id {} and got DataDevice with local id {}, data source with local id {}",
            mgr_local.local_id,
            seat_local.local_id,
            local_data_device.local_id,
            data_source.local_id
        );

        debug_println!("Read input data: {:?}", in_str);

        display.roundtrip_sync(&mut stream).unwrap();

        loop {
            display
                .dispatch_messages(&mut stream, |header, buffer, fds, offset| {
                    debug_println!(
                        "Received message for object_id {} with opcode {}",
                        header.object_id,
                        header.opcode
                    );
                    if let Some(event) = data_source.parse_message(header, buffer, fds, offset) {
                        match event {
                            #[cfg_attr(not(debug_assertions), expect(unused_variables))]
                            WlDataControlSourceEvent::Send { mime_type, fd } => {
                                debug_println!(
                                    "Received send event with mime_type {} and fd {}",
                                    mime_type,
                                    fd
                                );

                                // Serve to pasting client via fd it sent.
                                fd.fd_write_and_close(&in_str).unwrap();
                            }
                            WlDataControlSourceEvent::Cancelled => {
                                debug_println!("Received cancelled event. Exiting...");
                                exit(0);
                            }
                        }
                    }
                })
                .unwrap();
        }
    } else {
        eprintln!("error: this compositor does not support ext_data_control_manager_v1");
        exit(1);
    }
}
