# Clip-For-Fun - a Wayland Clipboard Manager (WIP)

A pure Rust Wayland clipboard toolkit: `copy`, `paste`, and a planned `history` daemon.\
Inspired by [wl-clipboard](https://github.com/bugaevc/wl-clipboard).

The core library, `copy` and `paste` have zero dependencies - no libc crate or Wayland libraries. They talk the Wayland wire protocol directly over the Unix socket, with custom bindings for `sendmsg`/`recvmsg`. Only the `history` daemon will use crates (SQLite, image decoding, hashing).

## What works

- `clip-for-fun-copy`: copy arguments (joined with spaces) or stdin - Ctrl+V in other apps works, including X11 apps under Xwayland (`UTF8_STRING`, `STRING`, `TEXT`)
  - `-t`/`--type <mime>` offers the data as a specific MIME type, `-p`/`--primary` sets the primary (middle-click) selection
  - stdin up to 128 KiB stays in memory; larger input goes to an unnamed temp file (`O_TMPFILE`, falling back to create + immediate unlink) in `--temp-dir`, `$TMPDIR` or `/tmp`, so memory stays flat (a 100 MiB copy uses ~2.3 MB RSS) and nothing is left behind if the process is killed
  - returns as soon as the compositor confirms the selection (~2 ms), so `copy x && paste` always sees `x`; a background process keeps serving until something else takes ownership of the compositor selection. It runs in its own session from `/` with stdio on `/dev/null`, so it survives its terminal closing and never blocks `$(copy x)` or pipes. `-f`/`--foreground` stays in the foreground instead
- `clip-for-fun-paste`: paste to stdout, picking the best text MIME type the clipboard owner offers, streamed zero-copy with `splice` (via `std::io::copy`) when stdout is a file or pipe
- Wayland wire format (headers, ints, strings, new_id)
- Registry + binding `wl_seat` and `ext_data_control_manager_v1` at the lower of the server's and the implemented version
- Sending and receiving fds over the socket (`SCM_RIGHTS`)

## Platforms

Linux only. CI builds and tests on x86_64 and aarch64, and type-checks i686, armv7, powerpc64le, s390x, riscv64, loongarch64, sparc64 and x86_64 musl (architecture-specific constants such as `O_TMPFILE` and `SOL_SOCKET` differ between them).

## Usage

Needs a compositor that supports `ext_data_control_manager_v1` (developed and tested on Hyprland).

```sh
cargo build --release

# copy an argument
./target/release/clip-for-fun-copy "hello world"

# or copy piped input
cat file.txt | ./target/release/clip-for-fun-copy

# copy an image under its MIME type
./target/release/clip-for-fun-copy -t image/png < screenshot.png

# copy to the primary selection (middle-click paste)
./target/release/clip-for-fun-copy -p "hello"

# stay in the foreground and serve until something else is copied (Ctrl+C clears the clipboard)
./target/release/clip-for-fun-copy -f "hello"

# keep large input on disk instead of /tmp (often RAM-backed)
./target/release/clip-for-fun-copy --temp-dir ~/.cache < big.iso

# copy text that would otherwise be read as a flag
./target/release/clip-for-fun-copy -- "--primary"

# paste to stdout
./target/release/clip-for-fun-paste
```

Then paste anywhere. `copy` returns right away and a background process keeps serving the data until something else is copied, since on Wayland the owner of the clipboard has to serve the data itself. Without `--type`, input is offered as text.

`paste` only handles text for now and exits with an error if the clipboard has no text type.

For debug output, build without `--release`, run the binaries from `./target/debug/` instead, and pass `-f` to `copy` so the serving process keeps its stderr.

## Layout

| Crate | Contents |
| --- | --- |
| `core/` (`clip-for-fun-core`) | Wayland protocol: socket + fd passing, message reader/writer, object id routing, session setup (registry, seat, data-control device) |
| `copy/` | `clip-for-fun-copy` |
| `paste/` | `clip-for-fun-paste` |
| `history/` (planned) | `clip-for-fun-history` daemon + queries |

## Todo

- `paste`
  - Trailing newline only when printing text to a terminal, `-n` to suppress
  - `--type` and `--primary`
  - Bigger pipe buffer (`F_SETPIPE_SZ`) for faster large pastes
  - Non-text types, MIME type from the output file name
- Integration tests: the real binaries against a headless compositor, in CI
- `history`
  - Daemon that watches the clipboard and saves every copy to disk, all of its MIME types, without blocking the Wayland connection
  - Keeps the clipboard alive after the source app exits or clears it
  - Restore any older entry to the clipboard
  - Case-insensitive search over the full content, not just previews
- `copy`
  - `--file`: copy files the way a file manager does, so they paste into Nautilus, Dolphin or an upload dialog
  - MIME type inferred from the input
  - Zero-copy serving with `splice`, several pastes at once
- Fallback to `zwlr_data_control_manager_v1` (river, Wayfire, older sway)
- Benchmarks
- Packaging for the major distros
