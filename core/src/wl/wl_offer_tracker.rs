use std::{io, ops::ControlFlow, thread::ThreadId};

use crate::{
    DataControlDeviceEvent, DataControlOfferEvent, WlDataControlOffer, WlEvent, WlMessageRouter,
    WlSessionManager, log_debug,
    wl::wl_message_router::WlInterface::{self, DataControlOffer},
};

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
    offers: [Option<WlOffer>; 8], // Something higher than 2 as the compositor sends offers before selection.
    selection_id: Option<u32>,
    primary_selection_id: Option<u32>,
}

impl Default for WlOfferTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl WlOfferTracker {
    pub fn new() -> Self {
        Self {
            offers: [None, None, None, None, None, None, None, None],
            selection_id: None,
            primary_selection_id: None,
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

    pub fn get_primary_selected_slot(&self) -> Option<&WlOffer> {
        if let Some(primary_selection_id) = self.primary_selection_id {
            self.offers
                .iter()
                .flatten()
                .find(|offer| offer.id == primary_selection_id)
        } else {
            None
        }
    }

    pub fn data_device_offer(&mut self, new_id: u32) -> Result<(), std::io::Error> {
        log_debug!(
            "Received DataControlDevice DataOffer with new_id {}",
            new_id
        );

        let candidate = self.offers.iter_mut().find(|offer| offer.is_none());
        if let Some(slot) = candidate {
            *slot = Some(WlOffer::new(new_id));
        } else {
            return Err(io::Error::other("No available slot for new DataOffer"));
        }
        Ok(())
    }

    pub fn data_control_offer(&mut self, id: u32, mime: &[u8]) {
        log_debug!(
            "Received DataControlOffer with id {} and mime_type {:?}",
            id,
            String::from_utf8_lossy(mime)
        );

        let Some(rank) = KNOWN_MIME_TYPES.iter().position(|m| m.as_bytes() == mime) else {
            return;
        };
        let Some(best_offer) = self
            .offers
            .iter_mut()
            .flatten()
            .find(|offer| offer.id == id)
        else {
            return;
        };
        if best_offer.preferred_rank.is_some_and(|b| b <= rank) {
            return;
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

        return;
    }

    pub fn selection(&mut self, offer_id: &Option<u32>) -> Result<Option<u32>, std::io::Error> {
        log_debug!(
            "Received DataControlDevice Selection with offer_id {:?}",
            offer_id
        );
        let previous_selection = self.selection_id;
        self.selection_id = *offer_id;

        Ok(previous_selection)
    }

    pub fn primary_selection(
        &mut self,
        offer_id: &Option<u32>,
    ) -> Result<Option<u32>, std::io::Error> {
        log_debug!(
            "Received DataControlDevice PrimarySelection with offer_id {:?}",
            offer_id
        );
        let previous_primary_selection = self.primary_selection_id;
        self.primary_selection_id = *offer_id;

        Ok(previous_primary_selection)
    }

    pub fn remove(&mut self, offer_id: u32) {
        if let Some(slot) = self
            .offers
            .iter_mut()
            .find(|o| o.as_ref().is_some_and(|o| o.id == offer_id))
        {
            *slot = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: u32 = 0xff00_0000;
    const B: u32 = 0xff00_0001;

    fn data_offer(t: &mut WlOfferTracker, id: u32) {
        t.data_device_offer(id).unwrap();
    }

    fn offer(t: &mut WlOfferTracker, id: u32, mime: &str) {
        t.data_control_offer(id, mime.as_bytes());
    }

    fn select(t: &mut WlOfferTracker, offer_id: Option<u32>) -> Option<u32> {
        t.selection(&offer_id).unwrap()
    }

    fn selected_mime(t: &WlOfferTracker) -> Option<&'static str> {
        t.get_selected_slot()?.preferred_mime()
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
            offer(&mut t, A, mime);
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
            offer(&mut t, A, mime);
        }
        assert_eq!(selected_mime(&t), None);

        offer(&mut t, A, "TEXT");
        assert_eq!(selected_mime(&t), Some("TEXT"));
    }

    #[test]
    fn clipboard_and_primary_offers_are_tracked_separately() {
        let mut t = WlOfferTracker::new();
        data_offer(&mut t, A);
        offer(&mut t, A, "STRING");
        select(&mut t, Some(A));
        data_offer(&mut t, B);
        offer(&mut t, B, "text/plain;charset=utf-8");
        assert_eq!(t.primary_selection(&Some(B)).unwrap(), None);

        assert_eq!(selected_mime(&t), Some("STRING"));
        assert_eq!(
            t.get_primary_selected_slot()
                .and_then(|o| o.preferred_mime()),
            Some("text/plain;charset=utf-8")
        );
    }

    #[test]
    fn a_new_selection_returns_the_previous_one() {
        let mut t = WlOfferTracker::new();
        assert_eq!(select(&mut t, Some(A)), None);
        assert_eq!(select(&mut t, Some(B)), Some(A));
        assert_eq!(select(&mut t, None), Some(B));
        assert_eq!(
            t.primary_selection(&Some(A)).unwrap(),
            None,
            "primary has its own previous offer"
        );
    }

    #[test]
    fn removed_offers_free_their_slot_and_a_reused_id_starts_fresh() {
        let mut t = WlOfferTracker::new();
        // Far more offers than slots, each removed after use: what a long-running copy sees.
        for _ in 0..100 {
            data_offer(&mut t, A);
            offer(&mut t, A, "TEXT");
            select(&mut t, Some(A));
            t.remove(A);
        }
        data_offer(&mut t, A);
        select(&mut t, Some(A));
        assert_eq!(selected_mime(&t), None, "mime of the removed offer leaked");
    }

    #[test]
    fn running_out_of_slots_is_an_error() {
        let mut t = WlOfferTracker::new();
        let slots = t.offers.len() as u32;
        for i in 0..slots {
            data_offer(&mut t, A + i);
        }
        assert!(t.data_device_offer(A + slots).is_err());
    }

    #[test]
    fn empty_selection_has_no_selected_slot() {
        let mut t = WlOfferTracker::new();
        assert!(t.get_selected_slot().is_none());
        data_offer(&mut t, A);
        offer(&mut t, A, "text/plain");
        select(&mut t, None);
        assert!(t.get_selected_slot().is_none());
    }
}
