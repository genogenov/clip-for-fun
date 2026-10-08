use std::fmt::Display;
use std::io::Result;
use std::{ffi::OsString, path::PathBuf};

use clip_for_fun_core::{Colors, LOGGER, log_debug};

#[derive(Debug)]
pub struct PasteArgs {
    pub newline: bool,
    pub mime: Option<String>,
    pub primary: bool,
    pub list_types: bool,
}
pub enum Command {
    Help(String),
    Paste(PasteArgs),
    ListTypes,
}

pub fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<Command> {
    let mut args = args.into_iter();
    let program = args.next().unwrap_or_default();
    let mut out = PasteArgs {
        newline: false,
        mime: None,
        primary: false,
        list_types: false,
    };
    while let Some(arg) = args.next() {
        match arg.as_encoded_bytes() {
            b"-h" | b"--help" => return Ok(Command::Help(usage(&program))),
            b"-p" | b"--primary" => out.primary = true,
            b"-l" | b"--list-types" => {
                out.list_types = true;
                break;
            },
            b"-n" | b"--newline" => out.newline = true,
            b"-t" | b"--type" => {
                out.mime = Some(parse_mime(args.next(), &program, &arg.display())?)
            }
            _ => {
                return Err(usage_error(
                    &program,
                    &format!("unknown option '{}'", arg.display()),
                ));
            }
        }
    }

    log_debug!("Args parsed: {:?}", out);

    if out.list_types{
        return Ok(Command::ListTypes);
    }

    Ok(Command::Paste(out))
}

fn usage_error(program_cmd: &OsString, error: &str) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        format!("{}\n{}", error, usage(program_cmd),),
    )
}

pub(crate) fn usage(program_cmd: &OsString) -> String {
    let Colors {
        bold, cyan, reset, ..
    } = &LOGGER.colors;
    format!(
        concat!(
            "\n",
            "Usage:\n",
            "    {program} {bold}[options]{reset}\n",
            "Options:\n",
            "    {bold}-t, --type {cyan}<mime>{reset}           paste as this MIME type if its offered.\n",
            "    {bold}-p, --primary{reset}               paste from the primary selection instead of the clipboard\n",
            "    {bold}-n, --newline{reset}                append a newline to the pasted content if its text\n",
            "    {bold}-l, --list-types{reset}            list available MIME types\n",
            "    {bold}-h, --help{reset}                  show this help\n",
        ),
        program = program_cmd.display(),
        bold = bold,
        cyan = cyan,
        reset = reset,
    )
}

pub(crate) fn fallback_temp_dir() -> PathBuf {
    std::env::var_os("TMPDIR")
        .map(|v| {
            if v.is_empty() {
                PathBuf::from("/tmp")
            } else {
                PathBuf::from(v)
            }
        })
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn value(next: Option<OsString>, program: &OsString, option: impl Display) -> Result<OsString> {
    next.ok_or_else(|| {
        usage_error(
            program,
            &format!("expected a value for option '{}'", option),
        )
    })
}

fn parse_mime(next: Option<OsString>, program: &OsString, option: &impl Display) -> Result<String> {
    let value = value(next, program, option)?.into_string().map_err(|_| {
        usage_error(
            program,
            &format!(
                "invalid mime type for option '{}' - only UTF-8 strings are allowed",
                option
            ),
        )
    })?;

    if value.is_empty()
        || value.len() > 255
        || value.as_bytes().iter().any(|b| !b.is_ascii_graphic())
    {
        return Err(usage_error(
            program,
            &format!(
                "invalid mime type for option '{}' - must be non-empty, at most 255 characters, and contain only ASCII graphic characters",
                option
            ),
        ));
    }
    Ok(value)
}
