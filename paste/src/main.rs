use std::{
    env,
    ffi::OsString,
    io::{self, ErrorKind},
    ops::ControlFlow,
    path::PathBuf,
    process::ExitCode,
};

use clip_for_fun_core::{
    Colors, LOGGER, WlBufferedStream, WlDataControlOffer, WlEvent, WlSessionManager, log_debug,
    log_error,
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
    let args: Vec<OsString> = env::args_os().collect();
    parse_args(&args)?;

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

    log_debug!("Starting Paste loop");
    mgr.sync()?;

    mgr.dispatch_messages(&mut |ev| match ev {
        WlEvent::SyncDone => ControlFlow::Break(Ok(())),
        _ => ControlFlow::Continue(()),
    })?;

    let Some(slot) = mgr.get_selected_slot() else {
        return Err(io::Error::other("nothing found to paste"));
    };
    log_debug!(
        "Selected offer: id = {}, mime = {:?}",
        slot.id(),
        slot.preferred_mime()
    );

    let Some(preferred_mime) = slot.preferred_mime() else {
        return Err(io::Error::other("clipboard has no text content."));
    };

    let (mut reader, writer) = io::pipe()?;
    WlDataControlOffer::new(slot.id()).receive(
        mgr.get_message_writer(),
        preferred_mime,
        writer.into(),
    )?;
    mgr.send_messages()?;

    match io::copy(&mut reader, &mut io::stdout().lock()) {
        Err(e) if e.kind() == ErrorKind::BrokenPipe => Ok(()),
        other => other.map(drop),
    }
}

fn parse_args(args: &[OsString]) -> io::Result<()> {
    // This method is just a placeholder for future argument parsing logic.
    if args.len() <= 1 {
        return Ok(());
    }
    let Colors {
        bold,
        yellow,
        green,
        reset,
        ..
    } = LOGGER.colors;
    Err(io::Error::new(
        ErrorKind::InvalidInput,
        format!(
            "unexpected argument '{}'\n\
             \n\
             {bold}{yellow}Usage:{reset}\n  {green}{}{reset}",
            args[1].display(),
            args[0].display()
        ),
    ))
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
    fn no_arguments_is_accepted() {
        assert!(parse_args(&args(&[b"paste"])).is_ok());
    }

    #[test]
    fn any_argument_is_an_error() {
        for list in [&[&b"paste"[..], b"--paste"][..], &[b"paste", b"a", b"b"]] {
            let result = parse_args(&args(list));
            assert!(matches!(result, Err(e) if e.kind() == ErrorKind::InvalidInput));
        }
    }
}
