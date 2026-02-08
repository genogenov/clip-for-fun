mod wl;

use std::{arch::asm, env, io::{self, Write}};

use crate::wl::wl_socket::WLSocket;

fn main() {

    let socket_name = env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".to_string());
    let xdg_runtime_dir = env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is not set");

    let socket_path = format!("{}/{}", xdg_runtime_dir, socket_name);
    println!("Wayland socket path: {}", socket_path);

    let mut soc = WLSocket::connect(&socket_path).expect("Could not connect to unix socket");
    println!("Successfully connected to the Wayland socket");

    // _ = soc.send_message(WLMessage::new(WLObject::Display, WL_GET_REGISTRY_OPCODE));

    _ = soc.get_registry();

    println!("Sent get_registry message to the Wayland socket");
    _ = io::stdout().flush();

    loop {
        unsafe {
            asm!("nop");
        }
    }
}
