use std::{
    io::{Read, Write},
    mem::MaybeUninit,
    os::unix::net::UnixStream,
    ptr,
};

use crate::wl::{
    wl_objects::{
        Display, DisplayEvent, DisplayOps, MessageHeader, Registry, RegistryEvents, RegistryInterface, RegistryOps, WLCallbackEvents, WLObject, WlRegistryEvent, WlRegistryGlobalInterface
    },
};

pub struct WLSocket {
    stream: UnixStream,
    current_object_id: u32,
    write_buffer: [u8; 1024],
    write_cursor: usize,
    read_buffer: [u8; 4096],
    read_cursor: usize,
}

impl WLSocket {
    pub fn connect(socket_path: &str) -> std::io::Result<Self> {
        let stream = UnixStream::connect(socket_path)?;
        Ok(Self {
            stream: stream,
            current_object_id: 1,
            write_buffer: [0u8; 1024],
            write_cursor: 0,
            read_buffer: [0u8; 4096],
            read_cursor: 0,
        })
    }

    pub fn bind_registry_interface(
        &mut self,
        registry: &Registry,
        interface: RegistryInterface,
    ) -> std::io::Result<()> {
        self.pack_message_header::<Registry>(
            RegistryOps::Bind,
            registry.type_id,
        )?;

        Ok(())
    }

    pub fn get_registry(
        &mut self,
        interface_to_find: WlRegistryGlobalInterface,
    ) -> std::io::Result<Registry> {
        let display = Display;
        self.pack_message_header::<Display>(
            DisplayOps::GetRegistry,
            Display::TYPE_ID,
        )?;

        let mut registry = Registry::new(self.current_object_id);

        self.pack_message_header::<Display>(
            DisplayOps::Sync,
            Display::TYPE_ID,
        )?;
        let callback_id = self.current_object_id;

        self.flush_write_buffer()?;

        let mut bytes_read = self.stream.read(&mut self.read_buffer)?;
        self.read_cursor = 0;

        while bytes_read > 0 {
            while (self.read_cursor + MessageHeader::WL_HEADER_SIZE as usize) <= bytes_read {
                let header_u64 = unsafe {
                    ptr::read_unaligned(
                        self.read_buffer.as_ptr().add(self.read_cursor) as *const u64
                    )
                };
                let header: MessageHeader = header_u64.into();

                if header.size > self.read_buffer.len() as u16
                    || header.size < MessageHeader::WL_HEADER_SIZE
                {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Message size {} invalid", header.size),
                    ));
                }
                if header.size as usize + self.read_cursor > bytes_read {
                    break;
                }

                if let Some(rev) = registry.add_interface(
                    &header,
                    &self.read_buffer,
                    self.read_cursor + MessageHeader::WL_HEADER_SIZE as usize,
                ) {
                    match &rev {
                        WlRegistryEvent::Global {
                            global_name: _,
                            interface,
                            version: _,
                        } => match interface {
                            Some(interface) if *interface == interface_to_find => {
                                println!("Found global device manager {:?}", rev);

                                // change this if we need more interfaces
                                return Ok(registry);
                            }
                            _ => {}
                        },
                    }
                }

                if let Some(display_event) = display.parse_message(
                    &header,
                    &self.read_buffer,
                    self.read_cursor + MessageHeader::WL_HEADER_SIZE as usize,
                ) {
                    match display_event {
                        DisplayEvent::Error { .. } => {
                            println!(
                                "Received error message from Wayland socket: {:?}",
                                display_event
                            );
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                "Received error message from Wayland socket",
                            ));
                        }
                    }
                }

                if header.object_id == callback_id && header.opcode == WLCallbackEvents::Done as u16
                {
                    println!("Received callback done event, registry enumeration complete");
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Registry enumeration complete, interface not found",
                    ));
                }

                self.read_cursor += header.size as usize;
                //sleep(Duration::from_secs(1));
            }

            // we may have read a partial message, so we need to move the remaining bytes to the beginning of the buffer
            let mut remaining_bytes = 0;
            if self.read_cursor < bytes_read {
                remaining_bytes = bytes_read - self.read_cursor;
                self.read_buffer
                    .copy_within(self.read_cursor..bytes_read, 0);
                // println!(
                //     "Moved {} remaining bytes to the beginning of the buffer for the next read",
                //     remaining_bytes
                // );
            }

            //sleep(Duration::from_secs(1));
            //println!("Waiting for more messages from the Wayland socket...");
            let new_bytes_read = self.stream.read(&mut self.read_buffer[remaining_bytes..])?;
            if new_bytes_read == 0 {
                break;
            }
            //println!("Read {} new bytes from the Wayland socket", new_bytes_read);
            bytes_read = remaining_bytes + new_bytes_read;
            self.read_cursor = 0;
        }

        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Failed to read from Wayland socket",
        ))
    }

    fn pack_message_header<T: WLObject>(
        &mut self,
        op: T::Ops,
        type_id: u32,
    ) -> Result<(), std::io::Error> {
        if (self.write_cursor + 12) > self.write_buffer.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Write buffer overflow",
            ));
        }
        self.current_object_id += 1;
        const MSG_SIZE: u16 = MessageHeader::WL_HEADER_SIZE + 4; // header + new_id
        let opcode: u16 = op.into();

        let buf = &mut self.write_buffer[self.write_cursor..self.write_cursor + 12];
        buf[0..4].copy_from_slice(&type_id.to_ne_bytes());
        buf[4..6].copy_from_slice(&opcode.to_ne_bytes());
        buf[6..8].copy_from_slice(&MSG_SIZE.to_ne_bytes());
        buf[8..12].copy_from_slice(&self.current_object_id.to_ne_bytes());

        self.write_cursor += 12;
        Ok(())
    }

    fn flush_write_buffer(&mut self) -> std::io::Result<()> {
        self.stream.write_all(&self.write_buffer[..self.write_cursor])?;
        self.write_cursor = 0;
        Ok(())
    }
}
