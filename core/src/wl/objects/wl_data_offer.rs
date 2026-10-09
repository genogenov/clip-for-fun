use std::{borrow::Cow, os::fd::OwnedFd};

use crate::{
    unix_fd_stream::InFdBuffer,
    wl::{
        objects::{WlObject, wl_enum},
        wl_message_reader::WlMessageReader,
        wl_message_writer::WlMessageWriter,
    },
};
use std::io;

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

pub const TEXT_PLAIN_UTF8: &str = "text/plain;charset=utf-8";
pub const UTF8_STRING: &str = "UTF8_STRING";
pub const TEXT_PLAIN: &str = "text/plain";
pub const STRING: &str = "STRING";
pub const TEXT: &str = "TEXT";
pub const TEXT_HTML: &str = "text/html";

// Ordered by preference: index == rank.
pub const KNOWN_MIME_TYPES: [&str; 6] = [
    TEXT_PLAIN_UTF8,
    UTF8_STRING,
    TEXT_PLAIN,
    STRING,
    TEXT,
    TEXT_HTML,
];
pub const OFFERED_TXT_MIME_TYPES: [&str; 5] =
    [TEXT_PLAIN_UTF8, UTF8_STRING, TEXT_PLAIN, STRING, TEXT];

pub struct WlDataControlOffer {
    id: u32,
    offered_mime_types: Vec<Cow<'static, str>>,
}
impl WlObject for WlDataControlOffer {
    type Ops = WlDataControlOfferOps;
    type Events = WlDataControlOfferEvents;
}

impl WlDataControlOffer {
    pub fn new(local_id: u32) -> Self {
        Self {
            id: local_id,
            offered_mime_types: Vec::with_capacity(8),
        }
    }

    pub fn preferred_mime_type(&self) -> Option<&Cow<'static, str>> {
        let ranks = self
            .offered_mime_types
            .iter()
            .map(|mime| {
                KNOWN_MIME_TYPES.iter().position(|m| m == mime).map_or_else(
                    || {
                        if mime.contains("text") {
                            (99, mime)
                        } else {
                            (999, mime)
                        }
                    },
                    |rank| (rank, mime),
                )
            })
            .min_by(|(rank_a, _), (rank_b, _)| rank_a.cmp(rank_b));

        ranks.map(|(_, mime)| mime)
    }

    pub fn asked_mime_type(&self, asked: &str) -> Option<&Cow<'static, str>> {
        self.offered_mime_types.iter().find(|m| *m == asked)
    }

    pub fn offered_mime_types(&self) -> &[Cow<'static, str>] {
        &self.offered_mime_types
    }

    pub fn push_offer(&mut self, mime: &[u8]) -> Result<(), io::Error> {
        if self.offered_mime_types.len() >= 32 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Too many offered mime types",
            ));
        }
        if let Some(known) = KNOWN_MIME_TYPES.iter().find(|m| m.as_bytes() == mime) {
            self.offered_mime_types.push(Cow::Borrowed(known));
        } else if self.offered_mime_types.iter().any(|m| m.as_bytes() == mime) {
            return Ok(());
        } else {
            self.offered_mime_types.push(Cow::Owned(
                str::from_utf8(mime)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
                    .to_owned(),
            ))
        }
        Ok(())
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn receive(
        id: u32,
        writer: WlMessageWriter,
        mime: &str,
        fd: OwnedFd,
    ) -> Result<(), std::io::Error> {
        let mut msg =
            writer.begin_message::<WlDataControlOffer>(WlDataControlOfferOps::Receive, id)?;
        msg.pack_str(mime)?;
        msg.pack_fd(fd)?;
        msg.end();
        Ok(())
    }

    pub fn destroy(id: u32, writer: WlMessageWriter) -> Result<(), std::io::Error> {
        let msg = writer.begin_message::<WlDataControlOffer>(WlDataControlOfferOps::Destroy, id)?;
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

impl PartialEq for WlDataControlOffer {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
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

        WlDataControlOffer::receive(
            offer_id,
            client.get_writer(),
            "text/plain",
            payload_tx.into(),
        )
        .unwrap();
        WlDataControlOffer::destroy(offer_id, client.get_writer()).unwrap();
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
