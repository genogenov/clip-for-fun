pub mod objects;
pub mod wl_buffered_stream;
pub mod wl_message_reader;
pub mod wl_message_router;

macro_rules! debug_println {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        {
            // Use eprint! or eprintln! if you want to print to stderr like dbg!
            eprintln!("DEBUG: {}", format!($($arg)*));
        }
    };
}

pub(crate) use debug_println;
