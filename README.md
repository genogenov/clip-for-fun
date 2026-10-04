# Clip-For-Fun - a Wayland Clipboard Manager (WIP)

A pure Rust Wayland clipboard toolkit: `copy`, `paste`, and a planned `history` daemon.\
Inspired by [wl-clipboard](https://github.com/bugaevc/wl-clipboard).

The core library, `copy` and `paste` have zero dependencies - no libc crate or Wayland libraries. They talk the Wayland wire protocol directly over the Unix socket, with custom bindings for `sendmsg`/`recvmsg`. Only the `history` daemon will use crates (SQLite, image decoding, hashing).

## What works

- `clip-for-fun-copy`: copy from an argument or stdin - Ctrl+V in other apps works, including X11 apps under Xwayland (`UTF8_STRING`, `STRING`, `TEXT`)
- `clip-for-fun-paste`: paste to stdout, picking the best text MIME type the clipboard owner offers, streamed zero-copy with `splice` (via `std::io::copy`) when stdout is a file or pipe
- Wayland wire format (headers, ints, strings, new_id)
- Registry + binding `wl_seat` and `ext_data_control_manager_v1` at the lower of the server's and the implemented version
- Sending and receiving fds over the socket (`SCM_RIGHTS`)

## Usage

Needs a compositor that supports `ext_data_control_manager_v1` (developed and tested on Hyprland).

```sh
cargo build --release

# copy an argument
./target/release/clip-for-fun-copy "hello world"

# or copy piped input
cat file.txt | ./target/release/clip-for-fun-copy

# copy text that would otherwise be read as a flag
./target/release/clip-for-fun-copy -- "--foreground"

# paste to stdout
./target/release/clip-for-fun-paste
```

Then paste anywhere. `copy` stays alive in the foreground until something else is copied, since on Wayland the owner of the clipboard has to serve the data itself.

`paste` only handles text for now and exits with an error if the clipboard has no text type.

For debug output, build without `--release` and run the binaries from `./target/debug/` instead.

## Layout

| Crate | Contents |
| --- | --- |
| `core/` (`clip-for-fun-core`) | Wayland protocol: socket + fd passing, message reader/writer, object id routing, session setup (registry, seat, data-control device) |
| `copy/` | `clip-for-fun-copy` |
| `paste/` | `clip-for-fun-paste` |
| `history/` (planned) | `clip-for-fun-history` daemon + queries |

## Todo

- `copy`
  - Run in the background after copying, like `wl-copy`
  - `--type` and `--primary`
  - MIME type inferred from the input
  - Zero-copy serving with `vmsplice`, several pastes at once
- `paste`
  - Trailing newline only when printing text to a terminal, `-n` to suppress
  - `--type` and `--primary`
  - MIME type from the output file name, non-text types
- `history`: a daemon that saves every copy to disk, keeps the clipboard alive after the source app exits, restores older entries and searches their full content
- Benchmarks
- Fallback to `zwlr_data_control_manager_v1`
