#![warn(clippy::undocumented_unsafe_blocks)]

#[doc(hidden)]
mod log_writer;
mod unix_fd_stream;
mod wl;

pub use wl::{
    objects::{
        wl_data_control_device::{DataControlDeviceEvent, WlDataControlDevice},
        wl_data_managers::DataDeviceManagerExt,
        wl_data_managers::ExtDataControlManagerV1,
        wl_data_offer::{DataControlOfferEvent, WlDataControlOffer},
        wl_data_source::WlDataControlSourceEvent,
        wl_display::WlDisplay,
        wl_registry::{BoundInterface, WlSeat},
    },
    wl_buffered_stream::WlBufferedStream,
    wl_message_reader::WlMessageReader,
    wl_message_router::{WlEvent, WlMessageRouter},
    wl_offer_tracker::{KNOWN_MIME_TYPES, OFFERED_TXT_MIME_TYPES, WlOffer, WlOfferTracker},
    wl_session_manager::WlSessionManager,
};

pub use log_writer::{Colors, LOGGER};
pub use unix_fd_stream::FdWriteAndClose;
