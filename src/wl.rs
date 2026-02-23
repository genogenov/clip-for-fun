pub mod wl_buffered_stream;
pub mod objects;

macro_rules! debug_println {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        {
            // Use eprint! or eprintln! if you want to print to stderr like dbg!
            eprintln!("DEBUG: {}", format!($($arg)*));
        }
    };
}

pub (crate) use debug_println;