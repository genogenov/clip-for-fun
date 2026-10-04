use std::{
    fmt,
    io::{self, IsTerminal},
    sync::LazyLock,
};

// Initialized on first use; color is decided once (honors NO_COLOR, see https://no-color.org).
pub static LOGGER: LazyLock<LogWriter> = LazyLock::new(LogWriter::new);

#[derive(Clone, Copy)]
pub struct Colors {
    pub bold: &'static str,
    pub red: &'static str,
    pub green: &'static str,
    pub yellow: &'static str,
    pub cyan: &'static str,
    pub reset: &'static str,
}

const ANSI: Colors = Colors {
    bold: "\x1b[1m",
    red: "\x1b[31m",
    green: "\x1b[32m",
    yellow: "\x1b[33m",
    cyan: "\x1b[36m",
    reset: "\x1b[0m",
};

const PLAIN: Colors = Colors {
    bold: "",
    red: "",
    green: "",
    yellow: "",
    cyan: "",
    reset: "",
};

pub struct LogWriter {
    pub colors: Colors,
    info_prefix: &'static str,
    error_prefix: &'static str,
    debug_prefix: &'static str,
}

impl LogWriter {
    fn new() -> Self {
        let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
        let colored = io::stderr().is_terminal() && !no_color;
        let pick = |color: &'static str, plain: &'static str| if colored { color } else { plain };
        Self {
            colors: if colored { ANSI } else { PLAIN },
            info_prefix: pick("\x1b[1;32minfo:\x1b[0m", "info:"),
            error_prefix: pick("\x1b[1;31merror:\x1b[0m", "error:"),
            debug_prefix: pick("\x1b[1;36mdebug:\x1b[0m", "debug:"),
        }
    }

    pub fn info(&self, args: fmt::Arguments<'_>) {
        eprintln!("{} {args}", self.info_prefix);
    }

    pub fn error(&self, args: fmt::Arguments<'_>) {
        eprintln!("{} {args}", self.error_prefix);
    }

    pub fn debug(&self, args: fmt::Arguments<'_>) {
        eprintln!("{} {args}", self.debug_prefix);
    }
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        $crate::LOGGER.info(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        $crate::LOGGER.error(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        {
            $crate::LOGGER.debug(format_args!($($arg)*));
        }
    };
}
