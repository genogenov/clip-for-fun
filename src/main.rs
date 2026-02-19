mod wl;

use std::{
    env,
    io::{self, Write},
};

use crate::wl::{objects::registry::Registry, wl_socket::WLSocket};

fn main() {
    let socket_name = env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".to_string());
    let xdg_runtime_dir = env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is not set");

    let socket_path = format!("{}/{}", xdg_runtime_dir, socket_name);
    println!("Wayland socket path: {}", socket_path);

    let mut soc = WLSocket::connect(&socket_path).expect("Could not connect to unix socket");
    println!("Successfully connected to the Wayland socket");

    // _ = soc.send_message(WLMessage::new(WLObject::Display, WL_GET_REGISTRY_OPCODE));

    let registry_id = soc.get_registry().unwrap();
    let mut registry = Registry::new(registry_id);

    soc.dispatch_messages(|header, buffer, offset| {
        registry.add_interface(header, buffer, offset);
    })
    .unwrap();

    println!("Got registry: {:?}", registry);

    if let Some(ext_data_control_manager) = &registry.ext_data_control_manager {
        println!(
            "Found ExtDataControlManagerV1({}) with id {} and version {}",
            ext_data_control_manager.interface_name.str,
            ext_data_control_manager.global_name,
            ext_data_control_manager.version
        );
        let mgr_local_id = soc
            .bind_registry_interface(registry_id, ext_data_control_manager)
            .unwrap();
        let seat_local_id = soc
            .bind_registry_interface(registry_id, registry.wl_seat.as_ref().unwrap())
            .unwrap();

        println!(
            "Bound ExtDataControlManagerV1 to local id {}, and WlSeat to local id {}",
            mgr_local_id, seat_local_id
        );
        soc.dispatch_messages(|header, buffer, offset| {
            println!(
                "Received message with header: {:?}, buffer length: {}, offset: {}",
                header,
                buffer.len(),
                offset
            );
        })
        .unwrap();
    }
    _ = io::stdout().flush();
}
