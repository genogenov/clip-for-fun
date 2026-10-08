mod args;
use std::{
    env, ffi::OsString, io::{self, ErrorKind, IsTerminal, Write}, ops::ControlFlow, path::PathBuf, process::ExitCode,
};

use clip_for_fun_core::{
    KNOWN_MIME_TYPES, OFFERED_TXT_MIME_TYPES, WlBufferedStream, WlDataControlOffer, WlEvent, WlOffer, WlSessionManager, log_debug, log_error,
};

use crate::args::{Command, PasteArgs};

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
    let input_args = env::args_os();
    let args = match args::parse_args(input_args)? {
        Command::Help(usage) => {
            let _ = io::stdout().write_all(usage.as_bytes());
            return Ok(());
        }
        Command::ListTypes => {
            // Handle listing types here if needed
            return Ok(());
        }
        Command::Paste(args) => args,
    };

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

    write_all_to_stdout(&args, &mut mgr)
}

fn write_all_to_stdout(args: &PasteArgs, mgr: &mut WlSessionManager) -> io::Result<()> {
    let Some(offer) = get_offer(args, mgr) else {
        return Err(io::Error::other("nothing found to paste"));
    };
    log_debug!(
        "Selected offer: id = {}, mime = {:?}",
        offer.id(),
        offer.preferred_mime()
    );

    let Some(preferred_mime) = offer.preferred_mime() else {
        return Err(io::Error::other("clipboard has no text content."));
    };

    let (mut reader, writer) = io::pipe()?;
    WlDataControlOffer::new(offer.id()).receive(
        mgr.get_message_writer(),
        preferred_mime,
        writer.into(),
    )?;
    mgr.send_messages()?;

    let stdout = io::stdout();
    let mut out_guard = stdout.lock();
    _ = match io::copy(&mut reader, &mut out_guard) {
        Err(e) if e.kind() == ErrorKind::BrokenPipe => Ok(()),
        other => other.map(drop),
    }?;

    if args.newline && is_text_mime(preferred_mime) {
        writeln!(out_guard)?;
    }

    Ok(())
}

fn get_offer<'a>(args: &'a PasteArgs, mgr: &'a WlSessionManager) -> Option<&'a WlOffer> {
    if args.primary {
        mgr.get_primary_offer()
    } else {
        mgr.get_offer()
    }
}

fn is_text_mime(mime: &str) -> bool {
    mime.starts_with("text/") || OFFERED_TXT_MIME_TYPES.contains(&mime)
}