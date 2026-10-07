use std::{
    fs::{self, File, OpenOptions},
    hash::{BuildHasher, Hasher, RandomState},
    io::{self, ErrorKind, Read, Seek, SeekFrom, Write},
    os::{fd::OwnedFd, unix::fs::OpenOptionsExt},
    path::Path,
    process,
};

use clip_for_fun_core::log_debug;

// The limit of a single terminal arg. the vast majority of use cases should fit within this size.
pub const MAX_INLINE_BYTES: usize = 128 << 10;

// O_TMPFILE = __O_TMPFILE | O_DIRECTORY; differ between architectures.
#[cfg(any(
    target_arch = "aarch64",
    target_arch = "arm",
    target_arch = "powerpc",
    target_arch = "powerpc64",
    target_arch = "m68k"
))]
const O_TMPFILE: i32 = (1 << 22) | (1 << 14);
#[cfg(any(target_arch = "sparc", target_arch = "sparc64"))]
const O_TMPFILE: i32 = (1 << 25) | (1 << 16);
#[cfg(not(any(
    target_arch = "aarch64",
    target_arch = "arm",
    target_arch = "powerpc",
    target_arch = "powerpc64",
    target_arch = "m68k",
    target_arch = "sparc",
    target_arch = "sparc64"
)))]
const O_TMPFILE: i32 = (1 << 22) | (1 << 16); // kernel default: x86, riscv, loongarch, s390x, mips

pub enum Payload {
    Bytes(Vec<u8>),
    File { file: File },
}

impl Payload {
    pub fn serve(&self, out: OwnedFd) -> io::Result<()> {
        let mut out = File::from(out);
        match self {
            Payload::Bytes(bytes) => out.write_all(bytes),
            Payload::File { file, .. } => {
                let mut file: &File = file;
                file.seek(SeekFrom::Start(0))?;
                io::copy(&mut file, &mut out).map(drop)
            }
        }
    }
}

pub fn store(input: &mut File, temp_dir: &Path) -> io::Result<Payload> {
    // Try and see if we can fit in memory within the MAX_INLINE_BYTES limit.
    let mut head = Vec::new();
    input
        .take(MAX_INLINE_BYTES as u64 + 1)
        .read_to_end(&mut head)?;
    if head.len() <= MAX_INLINE_BYTES {
        log_debug!("Stored {} bytes in memory", head.len());
        return Ok(Payload::Bytes(head));
    }

    // If we reach here, the input is larger than MAX_INLINE_BYTES, so we store it in a temporary file.
    let mut file = open_tmpfile(temp_dir)?;
    file.write_all(&head)?;
    let rest = io::copy(input, &mut file)?;
    log_debug!(
        "Stored {} bytes in temporary file",
        head.len() + rest as usize
    );
    Ok(Payload::File {
        file
    })
}

fn open_tmpfile(dir: &Path) -> io::Result<File> {
    match OpenOptions::new()
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(O_TMPFILE)
        .open(dir)
    {
        Err(e) if needs_fallback(&e) => {
            log_debug!(
                "Falling back to unlinked temporary file due to O_TMPFILE not supported: {}",
                e
            );
            open_unlinked(dir)
        }
        other => {
            log_debug!("Created temporary file with O_TMPFILE successfully");
            other
        }
    }
    .map_err(|e| {
        io::Error::new(
            e.kind(),
            format!(
                "cannot create a temporary file in {}: {} (use --temp-dir)",
                dir.display(),
                e
            ),
        )
    })
}

// EOPNOTSUPP: filesystem without O_TMPFILE. EISDIR: old kernel. EINVAL: wrong flag value for this arch.
fn needs_fallback(e: &io::Error) -> bool {
    match e.kind() {
        ErrorKind::Unsupported | ErrorKind::IsADirectory | ErrorKind::InvalidInput => true,
        _ => false,
    }
}

fn open_unlinked(dir: &Path) -> io::Result<File> {
    for _ in 0..5 {
        let path = dir.join(format!(".cff-{}-{:016x}", process::id(), random_suffix()));
        match OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(file) => {
                // this uses unlink on UNIX, making the file nameless immediately after creation
                log_debug!(
                    "Created temporary file at {}. Unlinking it immediately.",
                    path.display()
                );
                fs::remove_file(&path)?;
                return Ok(file);
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                log_debug!("Temporary file name already exists, retrying...");
                continue;
            }
            Err(e) => {
                log_debug!("Failed to create temporary file: {}", e);
                return Err(e);
            }
        }
    }
    Err(io::Error::new(
        ErrorKind::AlreadyExists,
        "no free temporary file name",
    ))
}

fn random_suffix() -> u64 {
    RandomState::new().build_hasher().finish()
}
