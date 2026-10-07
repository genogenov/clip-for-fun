mod args;
use std::{
    env,
    ffi::OsString,
    io::{self, ErrorKind, Read, Write, stdin},
    ops::ControlFlow,
    os::unix::ffi::OsStringExt,
    path::PathBuf,
    process::ExitCode,
};

use clip_for_fun_core::{
    FdWriteAndClose, OFFERED_TXT_MIME_TYPES, WlBufferedStream, WlDataControlSourceEvent, WlEvent,
    WlSessionManager, log_debug, log_error,
};

use crate::args::{Command, Input};

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
        Command::Copy(args) => args,
    };
    if args.files || args.temp_dir.is_some() {
        return Err(io::Error::new(
            ErrorKind::Unsupported,
            "--file and --temp-dir are not implemented yet",
        ));
    }
    let input = match args.input {
        Input::Args(words) => words
            .into_iter()
            .map(OsString::into_vec)
            .collect::<Vec<_>>()
            .join(&b' '),
        Input::Stdin => {
            let mut input = Vec::new();
            stdin().read_to_end(&mut input)?;
            input
        }
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

    log_debug!("Read {} bytes of input", input.len());

    let data_source = mgr.create_data_source()?;
    if let Some(mime) = args.mime {
        data_source.offer(mgr.get_message_writer(), &mime)?;
    } else {
        for mime in OFFERED_TXT_MIME_TYPES {
            data_source.offer(mgr.get_message_writer(), mime)?;
        }
    }

    if args.primary {
        mgr.set_primary_selection(data_source.local_id)?;
    } else {
        mgr.set_selection(data_source.local_id)?;
    }

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
            id: _,
            event: WlDataControlSourceEvent::Cancelled,
        } => {
            log_debug!("Received cancelled event. Exiting...");
            ControlFlow::Break(Ok(()))
        }
        _ => ControlFlow::Continue(()),
    })
}
