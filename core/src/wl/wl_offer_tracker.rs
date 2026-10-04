use std::{io, ops::ControlFlow};

use crate::{DataControlDeviceEvent, DataControlOfferEvent, WlEvent, log_debug};

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

pub struct WlOffer {
    id: u32,
    preferred_rank: Option<usize>,
}

impl PartialEq for WlOffer {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl WlOffer {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            preferred_rank: None,
        }
    }

    pub fn preferred_mime(&self) -> Option<&'static str> {
        self.preferred_rank.map(|r| KNOWN_MIME_TYPES[r])
    }

    pub fn id(&self) -> u32 {
        self.id
    }
}

pub struct WlOfferTracker {
    offers: [Option<WlOffer>; 2],
    selection_id: Option<u32>,
}

impl Default for WlOfferTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl WlOfferTracker {
    pub fn new() -> Self {
        Self {
            offers: [None, None],
            selection_id: None,
        }
    }

    pub fn get_selected_slot(&self) -> Option<&WlOffer> {
        if let Some(selection_id) = self.selection_id {
            self.offers
                .iter()
                .flatten()
                .find(|offer| offer.id == selection_id)
        } else {
            None
        }
    }

    pub fn handle_event(&mut self, event: &WlEvent) -> ControlFlow<Result<(), std::io::Error>> {
        match event {
            WlEvent::DataControlDevice(DataControlDeviceEvent::DataOffer { new_id }) => {
                log_debug!(
                    "Received DataControlDevice DataOffer with new_id {}",
                    new_id
                );

                let mut candidate = self.offers.iter_mut().find(|offer| offer.is_none());
                if let Some(slot) = candidate {
                    *slot = Some(WlOffer::new(*new_id));
                } else {
                    candidate = self
                        .offers
                        .iter_mut()
                        .find(|offer| offer.as_ref().unwrap().id != self.selection_id.unwrap_or(0));
                    if let Some(slot) = candidate {
                        *slot = Some(WlOffer::new(*new_id));
                    } else {
                        return ControlFlow::Break(Err(io::Error::other(
                            "No available slot for new DataOffer",
                        )));
                    }
                }

                ControlFlow::Continue(())
            }
            WlEvent::DataControlDevice(DataControlDeviceEvent::Selection { offer_id }) => {
                log_debug!(
                    "Received DataControlDevice Selection with offer_id {:?}",
                    offer_id
                );
                self.selection_id = *offer_id;
                ControlFlow::Continue(())
            }
            WlEvent::DataControlDevice(DataControlDeviceEvent::Finished) => {
                ControlFlow::Break(Err(io::Error::other(
                    "Received DataControlDevice Finished event",
                )))
            }
            WlEvent::DataControlOffer {
                id,
                event: DataControlOfferEvent::Offer { mime },
            } => {
                log_debug!(
                    "Received DataControlOffer with id {} and mime_type {:?}",
                    id,
                    String::from_utf8_lossy(mime)
                );

                let Some(rank) = KNOWN_MIME_TYPES.iter().position(|m| m.as_bytes() == *mime) else {
                    return ControlFlow::Continue(());
                };
                let Some(best_offer) = self
                    .offers
                    .iter_mut()
                    .flatten()
                    .find(|offer| offer.id == *id)
                else {
                    return ControlFlow::Continue(());
                };
                if best_offer.preferred_rank.is_some_and(|b| b <= rank) {
                    return ControlFlow::Continue(());
                }
                log_debug!(
                    "Offer {}: best mime {:?} -> {:?}",
                    id,
                    best_offer
                        .preferred_rank
                        .map_or("None", |b| KNOWN_MIME_TYPES[b]),
                    KNOWN_MIME_TYPES[rank],
                );
                best_offer.preferred_rank = Some(rank);

                ControlFlow::Continue(())
            }
            WlEvent::SyncDone => {
                log_debug!("Received SyncDone event from the Wayland compositor");
                ControlFlow::Break(Ok(()))
            }
            _ => ControlFlow::Continue(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: u32 = 0xff00_0000;
    const B: u32 = 0xff00_0001;
    const C: u32 = 0xff00_0002;

    fn data_offer(t: &mut WlOfferTracker, new_id: u32) {
        let event = WlEvent::DataControlDevice(DataControlDeviceEvent::DataOffer { new_id });
        assert!(t.handle_event(&event).is_continue());
    }

    fn offer(t: &mut WlOfferTracker, id: u32, mime: &[u8]) {
        let event = WlEvent::DataControlOffer {
            id,
            event: DataControlOfferEvent::Offer { mime },
        };
        assert!(t.handle_event(&event).is_continue());
    }

    fn select(t: &mut WlOfferTracker, offer_id: Option<u32>) {
        let event = WlEvent::DataControlDevice(DataControlDeviceEvent::Selection { offer_id });
        assert!(t.handle_event(&event).is_continue());
    }

    fn selected_mime(t: &WlOfferTracker) -> Option<&'static str> {
        t.get_selected_slot()?
            .preferred_rank
            .map(|r| KNOWN_MIME_TYPES[r])
    }

    #[test]
    fn keeps_the_best_mime_regardless_of_arrival_order() {
        let mut t = WlOfferTracker::new();
        data_offer(&mut t, A);
        for mime in [
            "text/html",
            "STRING",
            "text/plain;charset=utf-8",
            "text/plain",
        ] {
            offer(&mut t, A, mime.as_bytes());
        }
        select(&mut t, Some(A));
        assert_eq!(selected_mime(&t), Some("text/plain;charset=utf-8"));
    }

    #[test]
    fn only_exact_known_mimes_count() {
        let mut t = WlOfferTracker::new();
        data_offer(&mut t, A);
        select(&mut t, Some(A));
        for mime in [
            "image/png",
            "text/plain;charset=utf-16",
            "text",
            "text/plain;",
        ] {
            offer(&mut t, A, mime.as_bytes());
        }
        assert_eq!(selected_mime(&t), None);

        offer(&mut t, A, b"TEXT");
        assert_eq!(selected_mime(&t), Some("TEXT"));
    }

    #[test]
    fn primary_offer_mimes_do_not_leak_into_the_clipboard_offer() {
        let mut t = WlOfferTracker::new();
        data_offer(&mut t, A);
        offer(&mut t, A, b"STRING");
        select(&mut t, Some(A));
        data_offer(&mut t, B);
        offer(&mut t, B, b"text/plain;charset=utf-8");
        let primary = WlEvent::DataControlDevice(DataControlDeviceEvent::PrimarySelection {
            offer_id: Some(B),
        });
        assert!(t.handle_event(&primary).is_continue());

        assert_eq!(selected_mime(&t), Some("STRING"));
    }

    #[test]
    fn third_offer_replaces_the_slot_that_is_not_the_selection() {
        let mut t = WlOfferTracker::new();
        data_offer(&mut t, A);
        offer(&mut t, A, b"TEXT");
        select(&mut t, Some(A));
        data_offer(&mut t, B);
        data_offer(&mut t, C);

        let ids: Vec<u32> = t.offers.iter().flatten().map(|o| o.id).collect();
        assert!(ids.contains(&A) && ids.contains(&C) && !ids.contains(&B));
        assert_eq!(selected_mime(&t), Some("TEXT"));

        offer(&mut t, C, b"text/plain");
        select(&mut t, Some(C));
        assert_eq!(selected_mime(&t), Some("text/plain"));
    }

    #[test]
    fn empty_selection_has_no_selected_slot() {
        let mut t = WlOfferTracker::new();
        assert!(t.get_selected_slot().is_none());
        data_offer(&mut t, A);
        offer(&mut t, A, b"text/plain");
        select(&mut t, None);
        assert!(t.get_selected_slot().is_none());
    }

    #[test]
    fn sync_done_stops_and_finished_is_an_error() {
        let mut t = WlOfferTracker::new();
        assert!(matches!(
            t.handle_event(&WlEvent::SyncDone),
            ControlFlow::Break(Ok(()))
        ));
        let finished = WlEvent::DataControlDevice(DataControlDeviceEvent::Finished);
        assert!(matches!(
            t.handle_event(&finished),
            ControlFlow::Break(Err(_))
        ));
    }
}
