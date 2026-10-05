use std::{
    env,
    ffi::OsString,
    io::{self, ErrorKind, Read, stdin},
    ops::ControlFlow,
    os::unix::ffi::OsStringExt,
    path::PathBuf,
    process::ExitCode,
};

use clip_for_fun_core::{
    Colors, FdWriteAndClose, LOGGER, OFFERED_TXT_MIME_TYPES, WlBufferedStream,
    WlDataControlSourceEvent, WlEvent, WlSessionManager, log_debug, log_error,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            log_error!("{}", err);
            ExitCode::FAILURE
        }
    }
}

fn run() -> io::Result<()> {
    let mut args: Vec<OsString> = env::args_os().collect();
    let input = parse_input(&mut args)?;

    let runtime_dir = env::var_os("XDG_RUNTIME_DIR")
        .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "XDG_RUNTIME_DIR is not set"))?;

    let socket_path = PathBuf::from(&runtime_dir)
        .join(env::var_os("WAYLAND_DISPLAY").unwrap_or_else(|| OsString::from("wayland-0")));
    log_debug!("Wayland socket path: {}", socket_path.display());

    let stream = WlBufferedStream::connect(&socket_path).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("cannot connect to {}: {e}", socket_path.display()),
        )
    })?;
    log_debug!("Successfully connected to the Wayland socket");

    let mut mgr = WlSessionManager::initialize(stream)?;

    log_debug!("Read {} bytes of input", input.len());

    let data_source = mgr.create_data_source()?;
    // let primary_data_source = mgr.create_data_source()?;
    for mime in OFFERED_TXT_MIME_TYPES {
        data_source.offer(mgr.get_message_writer(), mime)?;
        // primary_data_source.offer(mgr.get_message_writer(), mime)?;
    }
    mgr.set_selection(data_source.local_id)?;
    // mgr.set_primary_selection(primary_data_source.local_id)?;

    mgr.sync()?;

    mgr.dispatch_messages(&mut |wl_event| match wl_event {
        WlEvent::DataControlSource {
            id: _,
            event: WlDataControlSourceEvent::Send { mime_type: _, fd },
        } => {
            log_debug!("Received send event with fd {:?}", fd);

            // Serve to pasting client via fd it sent.
            if fd.fd_write_and_close(&input).is_err() {
                log_error!("Failed to write to fd");
            }
            ControlFlow::Continue(())
        }
        WlEvent::DataControlSource {
            id:_,
            event: WlDataControlSourceEvent::Cancelled,
        } => {
            log_debug!("Received cancelled event. Exiting...");
            ControlFlow::Break(Ok(()))
        }
        _ => ControlFlow::Continue(()),
    })
}

fn parse_input(args: &mut Vec<OsString>) -> io::Result<Vec<u8>> {
    match args.len() {
        3 if args[1] == "--" => {
            log_debug!("Using argument after '--' as copy content");
            Ok(args.swap_remove(2).into_vec())
        }
        2 => {
            log_debug!("Using argument as copy content");
            Ok(args.swap_remove(1).into_vec())
        }
        1 => {
            log_debug!("Using stdin as copy content");
            let mut input = Vec::new();
            stdin().read_to_end(&mut input)?;
            Ok(input)
        }
        _ => {
            let Colors {
                bold,
                yellow,
                green,
                cyan,
                reset,
                ..
            } = LOGGER.colors;
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "too many arguments\n\
                 \n\
                 {bold}{yellow}Usage:{reset}\n  {green}{}{reset} [{cyan}<text>{reset} | -- {cyan}<text>{reset}]",
                    args[0].display()
                ),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{ffi::OsStr, os::unix::ffi::OsStrExt};

    fn args(list: &[&[u8]]) -> Vec<OsString> {
        list.iter()
            .map(|a| OsStr::from_bytes(a).to_os_string())
            .collect()
    }

    #[test]
    fn non_utf8_argument_is_copied_byte_for_byte() {
        let input = parse_input(&mut args(&[b"copy", b"\xff\xfe"]));
        assert!(matches!(input, Ok(bytes) if bytes == b"\xff\xfe"));
    }

    #[test]
    fn double_dash_copies_text_that_looks_like_a_flag() {
        let input = parse_input(&mut args(&[b"copy", b"--", b"--foreground"]));
        assert!(matches!(input, Ok(bytes) if bytes == b"--foreground"));
    }

    #[test]
    fn too_many_arguments_is_an_error() {
        for list in [
            &[&b"copy"[..], b"a", b"b"][..],
            &[b"copy", b"a", b"b", b"c"],
        ] {
            let input = parse_input(&mut args(list));
            assert!(matches!(input, Err(e) if e.kind() == ErrorKind::InvalidInput));
        }
    }
}
