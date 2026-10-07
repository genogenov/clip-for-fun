use std::{
    fmt,
    io::{self, IsTerminal, Write},
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

macro_rules! bold {
    () => {
        "\x1b[1m"
    };
}
macro_rules! red {
    () => {
        "\x1b[31m"
    };
}
macro_rules! green {
    () => {
        "\x1b[32m"
    };
}
macro_rules! yellow {
    () => {
        "\x1b[33m"
    };
}
macro_rules! cyan {
    () => {
        "\x1b[36m"
    };
}
macro_rules! reset {
    () => {
        "\x1b[0m"
    };
}

const ANSI: Colors = Colors {
    bold: bold!(),
    red: red!(),
    green: green!(),
    yellow: yellow!(),
    cyan: cyan!(),
    reset: reset!(),
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
            info_prefix: pick(concat!(bold!(), green!(), "info:", reset!()), "info:"),
            error_prefix: pick(concat!(bold!(), red!(), "error:", reset!()), "error:"),
            debug_prefix: pick(concat!(bold!(), cyan!(), "debug:", reset!()), "debug:"),
        }
    }

    pub fn info(&self, args: fmt::Arguments<'_>) {
        Self::write_line(self.info_prefix, args);
    }

    pub fn error(&self, args: fmt::Arguments<'_>) {
        Self::write_line(self.error_prefix, args);
    }

    pub fn debug(&self, args: fmt::Arguments<'_>) {
        Self::write_line(self.debug_prefix, args);
    }

    fn write_line(prefix: &str, args: fmt::Arguments<'_>) {
        let line = format!("{prefix} {args}\n");
        let _ = io::stderr().write_all(line.as_bytes());
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
