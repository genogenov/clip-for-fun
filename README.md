# Clip-For-Fun - a Wayland Clipboard Manager (WIP)

A Wayland clipboard toolkit: `copy`, `paste`, and a planned `history` daemon.\
Inspired by [wl-clipboard](https://github.com/bugaevc/wl-clipboard).

## What clip-for-fun is

Tools for the Wayland clipboard. Options and examples are under [Usage](#usage).

- **`clip-for-fun-copy`** puts text or any data on the clipboard or the primary (middle-click) selection. It returns immediately while a background process keeps the data available, so other apps - Wayland and X11 alike - can paste it until something else is copied. Large inputs don't fill up memory.
- **`clip-for-fun-paste`** writes the clipboard or primary selection to stdout. It picks the best text type by default, can paste any specific type, and lists what is on offer.
- **`clip-for-fun-history`** (planned) will remember every copy, keep the clipboard alive after the source app exits, and let you search and restore older entries, with a UI/TUI to browse them.

## Why

clip-for-fun aims to be one modern package for everything clipboard on current Wayland compositors - copy, paste, history and a UI/TUI - built on a single fast, dependency-free core and designed and tested together.

## Usage

Needs a compositor that supports `ext_data_control_manager_v1`. Build with `cargo build --release`; the binaries end up in `./target/release/`.

### `clip-for-fun-copy [options] [<text>...]`

| Option | Meaning |
| --- | --- |
| `<text>...` | Copy the arguments, joined with spaces. Without arguments, stdin is copied. |
| `-t`, `--type <mime>` | Offer the data under this MIME type (default: the usual text types). |
| `-p`, `--primary` | Set the primary (middle-click) selection instead of the clipboard. |
| `-f`, `--foreground` | Keep serving in the foreground instead of returning right away. |
| `--temp-dir <dir>` | Where stdin over 128 KiB is kept (default: `$TMPDIR`, then `/tmp`). |
| `--` | Treat everything after it as text, even if it looks like an option. |
| `-h`, `--help` | Show help. |

### `clip-for-fun-paste [options]`

| Option | Meaning |
| --- | --- |
| `-t`, `--type <mime>` | Paste exactly this MIME type. If it isn't offered, the error lists what is. |
| `-p`, `--primary` | Paste the primary selection instead of the clipboard. |
| `-l`, `--list-types` | List the offered MIME types instead of pasting. |
| `-n`, `--no-newline` | Never add a trailing newline. By default one is added only when printing text to a terminal. |
| `-h`, `--help` | Show help. |

Without `--type`, `paste` picks the best text type (`text/plain;charset=utf-8` first), or the first offered type if there is no text.

### Examples

```sh
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

# list the offered types, then paste one of them
./target/release/clip-for-fun-paste -l
./target/release/clip-for-fun-paste -t image/png > shot.png

# paste the primary selection without a trailing newline
./target/release/clip-for-fun-paste -p -n
```

On Wayland the owner of the clipboard has to serve the data itself, which is why `copy` leaves a background process running until something else is copied.

For debug output, build without `--release`, run the binaries from `./target/debug/` instead, and pass `-f` to `copy` so the serving process keeps its stderr.

## Technical principles

- **Zero dependencies** for the core library, `copy` and `paste`: no libc crate, no libwayland. Only the `history` daemon will use crates (SQLite, image decoding, hashing).
- **Hand-written Wayland wire protocol** over the Unix socket: message framing, object id routing, and file descriptor passing (`SCM_RIGHTS`) through hand-written `sendmsg`/`recvmsg` bindings.
- **Minimal `unsafe`**: limited to a small FFI module (`sendmsg`, `recvmsg`, `fork`, `fcntl`, ...), each call with a `SAFETY` comment. Architecture-specific constants are checked against the kernel headers.
- **No helper processes**: `copy` and `paste` read, serve and receive the data themselves without starting other programs, which keeps small copies and pastes fast.
- **Zero-copy where the kernel allows it**: pastes are streamed with `splice` through an enlarged pipe.
- **Bounded memory**: input over 128 KiB goes to an unnamed temp file (`O_TMPFILE`, or create + immediate unlink), so a 100 MiB copy uses ~2.3 MB of RAM and nothing is left on disk even if the process is killed.
- **A well-behaved background process**: `copy` forks only after the compositor confirms the selection, so `copy x && paste` always sees `x`. The child gets its own session, `/` as its working directory and `/dev/null` as stdio, so it survives its terminal closing and never blocks `$(copy x)` or pipes.
- **Other apps are untrusted**: MIME type names with control characters (terminal escape sequences) or invalid UTF-8 are skipped, offers are capped, and a malformed offer never stops `copy` or `paste`.

## Benchmarks

Coming soon.

## Platforms

Linux only. CI builds and tests on x86_64 and aarch64, and type-checks i686, armv7, powerpc64le, s390x, riscv64, loongarch64, sparc64 and x86_64 musl (architecture-specific constants such as `O_TMPFILE` and `SOL_SOCKET` differ between them).

## Layout

| Crate | Contents |
| --- | --- |
| `core/` (`clip-for-fun-core`) | Wayland protocol: socket + fd passing, message reader/writer, object id routing, session setup (registry, seat, data-control device) |
| `copy/` | `clip-for-fun-copy` |
| `paste/` | `clip-for-fun-paste` |
| `history/` (planned) | `clip-for-fun-history` daemon + queries |

## Todo

See [TODO.md](TODO.md).
