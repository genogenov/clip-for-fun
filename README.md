# Clip-For-Fun - a Wayland Clipboard Manager (for fun)
This is a pure Rust, zero dependency implementation of a clipboard manager (copying and pasting, no history)\
It is very much a work in progress\
The reason this project exists is 99.9% for practicing my Rust skills, and gaining some low level knowledge about Linux, Wayland, syscalls, etc.

No libc crate or wayland libraries - it talks the Wayland wire protocol directly over the unix socket, with my own bindings for `sendmsg`/`recvmsg`.

## What works
- Copy from an argument or stdin - Ctrl+V in other apps works
- Wayland wire format (headers, ints, strings, new_id)
- Registry + binding `wl_seat` and `ext_data_control_manager_v1`
- Receiving fds over the socket (`SCM_RIGHTS`)
- Reassembling messages split across socket reads

## Todo
- Paste mode (like `wl-paste`)
- Fallback to `zwlr_data_control_manager_v1`
- Run in the background after copying, like `wl-copy`
- CLI flags (`--paste`, `--type`, `--primary`)
- Proper error handling instead of `unwrap()`
- More tests

## Usage
Needs a compositor that supports `ext_data_control_manager_v1` (developed and tested on Hyprland).

```sh
cargo build --release

# copy an argument
./target/release/clip-for-fun "hello world"

# or copy piped input
cat file.txt | ./target/release/clip-for-fun
```

Then paste anywhere. The process stays alive until something else is copied, since on Wayland the owner of the clipboard has to serve the data itself.

For debug output, build without `--release` and run `./target/debug/clip-for-fun` instead.
