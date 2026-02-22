mod unix_fd_stream;
mod wl;

use std::{
    env,
    io::{self, Write},
};

use crate::wl::{
    objects::{wl_data_device::DataDeviceManagerExt, wl_display::WlDisplay},
    wl_buffered_stream::WLBufferedStream,
};

fn main() {
    let socket_name = env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".to_string());
    let xdg_runtime_dir = env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is not set");

    let socket_path = format!("{}/{}", xdg_runtime_dir, socket_name);
    println!("Wayland socket path: {}", socket_path);

    let mut stream =
        WLBufferedStream::connect(&socket_path).expect("Could not connect to unix socket");
    println!("Successfully connected to the Wayland socket");

    // _ = soc.send_message(WLMessage::new(WLObject::Display, WL_GET_REGISTRY_OPCODE));

    let mut display = WlDisplay::new();

    let mut registry = display.get_registry(&mut stream).unwrap();
    display.roundtrip_sync(&mut stream).unwrap();
    display
        .dispatch_messages(&mut stream, |header, buffer, offset| {
            registry.add_interface(header, buffer, offset);
        })
        .unwrap();

    println!("Got registry: {:?}", registry);

    if let Some(ext_data_control_manager) = registry.ext_data_control_manager.clone() {
        println!(
            "Found ExtDataControlManagerV1({}) with id {} and version {}",
            ext_data_control_manager.interface_name.str,
            ext_data_control_manager.global_name,
            ext_data_control_manager.version
        );

        let mgr_local = registry
            .bind(&mut stream, ext_data_control_manager.clone())
            .unwrap();
        let seat_local = registry
            .bind(&mut stream, registry.wl_seat.clone().unwrap())
            .unwrap();

        mgr_local
            .get_data_device(&mut stream, seat_local.local_id)
            .unwrap();

        println!(
            "Bound ExtDataControlManagerV1 to local id {}, and WlSeat to local id {}",
            mgr_local.local_id, seat_local.local_id
        );

        display.roundtrip_sync(&mut stream).unwrap();
        display
            .dispatch_messages(&mut stream, |header, buffer, offset| {
                println!(
                    "Received message with header: {:?}, buffer length: {}, offset: {}",
                    header,
                    buffer.len(),
                    offset
                );
            })
            .unwrap();
        // stream.get_data_device(mgr_local_id, seat_local_id).unwrap();
    }
    _ = io::stdout().flush();
}
