use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    ptr,
};

use crate::wl::{
    wl_message::WLMessage,
    wl_objects::{
        WL_DATA_DEVICE_MANAGER_NAME, WL_DISPLAY_EV_ERROR, WL_DISPLAY_OP_GET_REGISTRY, WL_DISPLAY_OP_SYNC, WL_HEADER_SIZE, WL_REGISTRY_CALLBACK_DONE, WL_REGISTRY_EV_GLOBAL, WLObject
    },
};

pub struct WLSocket {
    stream: UnixStream,
    current_object_id: u32,
}

impl WLSocket {
    pub fn connect(socket_path: &str) -> std::io::Result<Self> {
        let stream = UnixStream::connect(socket_path)?;
        Ok(Self {
            stream: stream,
            current_object_id: 1,
        })
    }

    pub fn get_registry(&mut self) -> std::io::Result<()> {
        _ = self.send_message(WLMessage::new(WLObject::Display, WL_DISPLAY_OP_GET_REGISTRY))?;
        _ = self.send_message(WLMessage {
            object_id: WLObject::Display,
            opcode: WL_DISPLAY_OP_SYNC,
        })?;
        let callback_id = self.current_object_id;

        let mut buffer = [0u8; 256];
        let mut bytes_read = self.stream.read(&mut buffer)?;
        let mut idx = 0;

        while bytes_read > 0 {
            //println!("Received {} bytes from the Wayland socket", bytes_read);

            while (idx + WL_HEADER_SIZE as usize) < bytes_read {
                //println!("Parsing message at buffer index {}", idx);
                let object_id = unsafe { ptr::read(buffer.as_ptr().add(idx) as *const u32) };
                let opcode = unsafe { ptr::read(buffer.as_ptr().add(idx + 4) as *const u16) };
                let size = unsafe { ptr::read(buffer.as_ptr().add(idx + 6) as *const u16) };

                if size as usize + idx > bytes_read {
                    // println!(
                    //     "Message size {} exceeds remaining buffer size {}, stopping parsing",
                    //     size,
                    //     bytes_read as u16 - idx as u16
                    // );
                    break;
                }

                // let size = (size_op >> 16) as usize;
                // let opcode = (size_op & 0xFFFF) as u16;

                // println!(
                //     "Received message with object ID {}, opcode {}, and size {}",
                //     object_id, opcode, size
                // );

                if object_id == WLObject::Registry as u32 && opcode == WL_REGISTRY_EV_GLOBAL {
                    let global_name =
                        unsafe { ptr::read(buffer.as_ptr().add(idx + 8) as *const u32) };
                    let interface_name_len =
                        unsafe { ptr::read(buffer.as_ptr().add(idx + 12) as *const u32) };

                    let interface_name = std::str::from_utf8(
                        &buffer[idx + 16..idx + 16 + interface_name_len as usize],
                    )
                    .unwrap();

                    if interface_name.trim_matches('\0') == WL_DATA_DEVICE_MANAGER_NAME {
                        println!(
                            "Found global device manager with name {} and ID {}",
                            interface_name, global_name
                        );
                    } 
                    // else {
                    //     println!(
                    //         "Found global with name {} and ID {}",
                    //         interface_name, global_name
                    //     );
                    // }
                }

                if object_id == WLObject::Display as u32 && opcode == WL_DISPLAY_EV_ERROR {
                    let tarhet_object_id =
                        unsafe { ptr::read(buffer.as_ptr().add(idx + 8) as *const u32) };
                    let error_code =
                        unsafe { ptr::read(buffer.as_ptr().add(idx + 12) as *const u32) };
                    let error_message_len =
                        unsafe { ptr::read(buffer.as_ptr().add(idx + 16) as *const u32) };
                    let error_message = std::str::from_utf8(
                        &buffer[idx + 20..idx + 20 + error_message_len as usize],
                    )
                    .unwrap();

                    println!(
                        "Received error message from Wayland socket: target object ID {}, error code {}, message {}",
                        tarhet_object_id, error_code, error_message
                    );
                }

                if object_id == callback_id && opcode == WL_REGISTRY_CALLBACK_DONE {
                    println!("Received callback done event, registry enumeration complete");
                    return Ok(());
                }

                idx += size as usize;
                //sleep(Duration::from_secs(1));
            }

            // we may have read a partial message, so we need to move the remaining bytes to the beginning of the buffer
            let mut remaining_bytes = 0;
            if idx < bytes_read {
                remaining_bytes = bytes_read - idx;
                buffer.copy_within(idx..bytes_read, 0);
                // println!(
                //     "Moved {} remaining bytes to the beginning of the buffer for the next read",
                //     remaining_bytes
                // );
            }

            //sleep(Duration::from_secs(1));
            //println!("Waiting for more messages from the Wayland socket...");
            let new_bytes_read = self.stream.read(&mut buffer[remaining_bytes..])?;
            //println!("Read {} new bytes from the Wayland socket", new_bytes_read);
            bytes_read = remaining_bytes + new_bytes_read;
            idx = 0;
        }

        Ok(())
    }

    pub fn send_message(&mut self, message: WLMessage) -> std::io::Result<()> {
        let msg_size: u16 = WL_HEADER_SIZE + size_of_val(&self.current_object_id) as u16;
        self.current_object_id += 1;
        let mut buffer = [0u8; 32];
        unsafe {
            ptr::write(buffer.as_mut_ptr() as *mut u32, message.object_id as u32);
            ptr::write(buffer.as_mut_ptr().add(4) as *mut u16, message.opcode);
            ptr::write(buffer.as_mut_ptr().add(6) as *mut u16, msg_size);
            ptr::write(
                buffer.as_mut_ptr().add(8) as *mut u32,
                self.current_object_id,
            );
        }

        self.stream.write_all(&buffer[0..msg_size as usize])
    }
}
