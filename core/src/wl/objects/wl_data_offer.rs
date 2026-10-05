use std::os::fd::OwnedFd;

use crate::{
    unix_fd_stream::InFdBuffer,
    wl::{
        objects::{WlObject, wl_enum},
        wl_message_reader::WlMessageReader,
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

    pub fn destroy(&self, writer: WlMessageWriter) -> Result<(), std::io::Error> {
        let msg = writer
            .begin_message::<WlDataControlOffer>(WlDataControlOfferOps::Destroy, self.local_id)?;
        msg.end();
        Ok(())
    }

    pub fn parse_message<'a>(
        opcode: u16,
        buffer: &'a [u8],
        _fds: &mut InFdBuffer,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FdWriteAndClose, wl::wl_buffered_stream::WlBufferedStream};
    use std::{io::Read, os::unix::net::UnixStream, time::Duration};

    #[test]
    fn receive_sends_the_mime_and_the_fd_and_destroy_has_no_arguments() {
        let (a, b) = UnixStream::pair().unwrap();
        let mut client = WlBufferedStream::new(a.into());
        let mut compositor = WlBufferedStream::new(b.into());
        let offer_id: u32 = 0xff00_0000;
        let (mut payload_rx, payload_tx) = UnixStream::pair().unwrap();

        let offer = WlDataControlOffer::new(offer_id);
        offer
            .receive(client.get_writer(), "text/plain", payload_tx.into())
            .unwrap();
        offer.destroy(client.get_writer()).unwrap();
        client.write().unwrap();

        let (header, body, fds) = compositor.read_next_message().unwrap().unwrap();
        assert_eq!(
            (header.object_id, header.opcode),
            (offer_id, u16::from(WlDataControlOfferOps::Receive))
        );
        assert_eq!(WlMessageReader::new(body).str(), Some(&b"text/plain"[..]));
        fds.pop_last_in_fd()
            .unwrap()
            .fd_write_and_close(b"hello")
            .unwrap();

        let (header, ..) = compositor.read_next_message().unwrap().unwrap();
        assert_eq!(
            (header.object_id, header.opcode, header.size),
            (offer_id, u16::from(WlDataControlOfferOps::Destroy), 8)
        );

        // Times out instead of hanging if the client kept its copy of the write end.
        payload_rx
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut got = Vec::new();
        payload_rx.read_to_end(&mut got).unwrap();
        assert_eq!(got, b"hello");
    }
}
