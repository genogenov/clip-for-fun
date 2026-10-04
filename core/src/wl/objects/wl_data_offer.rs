use std::os::fd::OwnedFd;

use crate::{
    unix_fd_stream::FdBuffer,
    wl::{
        objects::{WlObject, wl_enum},
        wl_buffered_stream::WlBufferedStream,
        wl_message_reader::WlMessageReader,
        wl_message_router::WlMessageRouter,
        wl_message_writer::WlMessageWriter,
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
        writer: WlMessageWriter,
        mime: &str,
        fd: OwnedFd,
    ) -> Result<(), std::io::Error> {
        let mut msg = writer
            .begin_message::<WlDataControlOffer>(WlDataControlOfferOps::Receive, self.local_id)?;
        msg.pack_str(mime)?;
        msg.pack_fd(fd)?;
        msg.end();
        Ok(())
    }

    pub fn destroy(
        &self,
        writer: WlMessageWriter,
        router: &mut WlMessageRouter,
    ) -> Result<(), std::io::Error> {
        let mut msg = writer
            .begin_message::<WlDataControlOffer>(WlDataControlOfferOps::Destroy, self.local_id)?;
        msg.end();
        router.free_server(self.local_id)
    }

    pub fn parse_message<'a>(
        opcode: u16,
        buffer: &'a [u8],
        _fds: &mut FdBuffer,
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
