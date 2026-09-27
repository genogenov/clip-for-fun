use std::os::fd::OwnedFd;

use crate::{
    unix_fd_stream::WlFdBuffer,
    wl::{
        objects::{WlObject, wl_enum},
        wl_buffered_stream::WlBufferedStream,
        wl_message_reader::WlMessageReader,
        wl_message_router::WlMessageRouter,
    },
};

const OFFER: u16 = 0;

wl_enum! {
    pub enum WlDataControlOfferEvents {
        Offer = OFFER
    }
}

wl_enum! {
    pub enum WlDataControlOfferOps {
        Receive = 0,

        Destroy = 1,
    }
}

pub enum DataControlOfferEvent<'a> {
    Offer { mime: &'a [u8] },
}

pub struct WlDataControlOffer {
    local_id: u32,
}
impl WlObject for WlDataControlOffer {
    type Ops = WlDataControlOfferOps;
    type Events = WlDataControlOfferEvents;
}

impl WlDataControlOffer {
    pub fn new(local_id: u32) -> Self {
        Self { local_id }
    }

    pub fn receive(
        &self,
        stream: &mut WlBufferedStream,
        mime: &str,
        fd: OwnedFd,
    ) -> Result<(), std::io::Error> {
        stream.pack_fd(fd)?;
        let start = stream
            .begin_message::<WlDataControlOffer>(WlDataControlOfferOps::Receive, self.local_id);
        stream.pack_str(mime);
        stream.end_message(start);
        Ok(())
    }

    pub fn destroy(
        &self,
        stream: &mut WlBufferedStream,
        router: &mut WlMessageRouter,
    ) -> Result<(), std::io::Error> {
        let start = stream
            .begin_message::<WlDataControlOffer>(WlDataControlOfferOps::Destroy, self.local_id);
        stream.end_message(start);
        router.free_server(self.local_id)
    }

    pub fn parse_message<'a>(
        opcode: u16,
        buffer: &'a [u8],
        _fds: &mut WlFdBuffer,
    ) -> std::io::Result<DataControlOfferEvent<'a>> {
        let mut reader = WlMessageReader::new(buffer);
        match opcode {
            OFFER => {
                let mime = reader.str().ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, "Failed to read mime")
                })?;
                Ok(DataControlOfferEvent::Offer { mime })
            }
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Unknown opcode",
            )),
        }
    }
}
