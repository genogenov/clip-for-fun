use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    ptr,
};

use crate::wl::{
    wl_message::WLMessage,
    wl_objects::{
        Display, DisplayEvent, DisplayOps, MessageHeader, Registry, RegistryEvents, RegistryOps,
        WLCallbackEvents, WLObject, WlRegistryEvent,
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

    pub fn bind_registry(&mut self) -> std::io::Result<()> {
        _ = self.send_message(WLMessage::<Registry> {
            opcode: RegistryOps::Bind,
        })?;

        Ok(())
    }

    pub fn get_registry(&mut self) -> std::io::Result<()> {
        _ = self.send_message(WLMessage::<Display> {
            opcode: DisplayOps::GetRegistry,
        })?;
        _ = self.send_message(WLMessage::<Display> {
            opcode: DisplayOps::Sync,
        })?;
        let callback_id = self.current_object_id;

        let mut buffer = [0u8; 256];
        let mut bytes_read = self.stream.read(&mut buffer)?;
        let mut idx = 0;

        while bytes_read > 0 {
            while (idx + MessageHeader::WL_HEADER_SIZE as usize) < bytes_read {
                let header = MessageHeader::deserialize(&buffer, idx);

                if header.size as usize + idx > bytes_read {
                    break;
                }

                if let Some(rev) = Registry::try_parse_event(
                    &header,
                    &buffer,
                    idx + MessageHeader::WL_HEADER_SIZE as usize,
                ) {
                    match rev {
                        WlRegistryEvent::Global {
                            global_name: _,
                            interface,
                            version: _,
                        } => {
                            if interface.trim_matches('\0') == Registry::WL_DATA_DEVICE_MANAGER_NAME
                            {
                                println!("Found global device manager {:?}", rev);
                            }
                            println!("Found global event {:?}", rev);
                        }
                    }
                }

                if let Some(display_event) = Display::try_parse_event(&header, &buffer, idx) {
                    match display_event {
                        DisplayEvent::Error { .. } => {
                            println!(
                                "Received error message from Wayland socket: {:?}",
                                display_event
                            );
                        }
                    }
                }

                if header.object_id == callback_id && header.opcode == WLCallbackEvents::Done as u16
                {
                    println!("Received callback done event, registry enumeration complete");
                    return Ok(());
                }

                idx += header.size as usize;
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

    pub fn send_message<T: WLObject>(&mut self, message: WLMessage<T>) -> std::io::Result<()> {
        let msg_size: u16 =
            MessageHeader::WL_HEADER_SIZE + size_of_val(&self.current_object_id) as u16;
        self.current_object_id += 1;
        let mut buffer = [0u8; 32];
        unsafe {
            ptr::write(buffer.as_mut_ptr() as *mut u32, T::TYPE_ID as u32);
            ptr::write(
                buffer.as_mut_ptr().add(4) as *mut u16,
                message.opcode.into(),
            );
            ptr::write(buffer.as_mut_ptr().add(6) as *mut u16, msg_size);
            ptr::write(
                buffer.as_mut_ptr().add(8) as *mut u32,
                self.current_object_id,
            );
        }

        self.stream.write_all(&buffer[0..msg_size as usize])
    }
}
