# Clip-For-Fun - a Wayland Clipboard Manager
This is a pure Rust, zero dependency implementation of a clipboard manager (copying and pasting, history to be implemented soon)\
It is very much a work in progress.

No libc crate or wayland libraries - it talks the Wayland wire protocol directly over the unix socket, with custom bindings for `sendmsg`/`recvmsg`.

## What works
- Copy from an argument or stdin - Ctrl+V in other apps works, including X11 apps under Xwayland (`UTF8_STRING`, `STRING`, `TEXT`)
- Paste to stdout (`--paste`), picking the best text MIME type the clipboard owner offers, streamed zero-copy with `splice` (via std::io::copy) when stdout is a file or pipe
- Wayland wire format (headers, ints, strings, new_id)
- Registry + binding `wl_seat` and `ext_data_control_manager_v1` at the lower of the server's and the implemented version
- Sending and receiving fds over the socket (`SCM_RIGHTS`)

## Todo
- Run in the background after copying, like `wl-copy`
- Benchmark against other popular copy/paste/clip history managers
- Watch mode + history support
- CLI flags (`--type`, `--primary`)
- MIME type guessing without spawning helpers like `xdg-mime`: on copy from the stdin file name or magic bytes (`clip-for-fun < shot.png` offers `image/png`), on paste from the stdout file name (`clip-for-fun --paste > shot.png` requests `image/png`)
- Append a trailing `\n` on paste only when stdout is a terminal, the MIME type is text, and the data doesn't already end with one (never for files or pipes), with `-n`/`--no-newline` to suppress
- Zero-copy serving on copy with `vmsplice`, and bigger pipes (`F_SETPIPE_SZ`) for large transfers
- Fallback to `zwlr_data_control_manager_v1`

## Usage
Needs a compositor that supports `ext_data_control_manager_v1` (developed and tested on Hyprland).

```sh
cargo build --release

# copy an argument
./target/release/clip-for-fun "hello world"

# or copy piped input
cat file.txt | ./target/release/clip-for-fun

# copy text that would otherwise be read as a flag (here: the literal text "--paste")
./target/release/clip-for-fun -- "--paste"

# paste to stdout
./target/release/clip-for-fun --paste
```

Then paste anywhere. The process stays alive until something else is copied, since on Wayland the owner of the clipboard has to serve the data itself.

`--paste` only handles text for now and exits with an error if the clipboard has no text type.

For debug output, build without `--release` and run `./target/debug/clip-for-fun` instead.
