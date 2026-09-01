//! Cross-process single-instance guard used by the two daemons (`spark-collect`
//! and `spark-serve`) on a shared database.
//!
//! The lock lives in the *database directory* as `.{name}.lock` (e.g.
//! `.collector.lock`, `.server.lock`). Using `flock(2)` gives us:
//!
//! * **1 collector per database** — the second `spark-collect` sees the lock
//!   held and refuses to start, so no two writers can ever share a DB.
//! * **1 server per database** — same idea for `spark-serve`; the second is
//!   refused regardless of `--port`.
//! * **N frontends** (local or on other machines) — untouched; frontends are
//!   plain HTTP clients of the single server, so any number of browsers at once
//!   is fine.
//! * **Auto-release on death** — `flock` is tied to an open file descriptor, so
//!   the kernel releases it the instant the process exits (cleanly, on `exit()`,
//!   by `SIGKILL`, by a crash, or by a container teardown). No "stuck lock" file
//!   can prevent a future start.
//! * **Different DBs = different lock files** — two daemons on two different
//!   databases can coexist; only sharing the same DB is refused.
//!
//! The lock file itself is intentionally *not* unlinked on drop; it's a small
//! marker that survives across restarts. Only the fd (and with it the lock)
//! is released when the owner goes away.

use std::fs::{File, OpenOptions};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

pub struct SingleInstanceLock {
    /// Kept alive for the lifetime of the process — dropping it closes the
    /// fd, which releases the `flock`.
    _file: File,
}

/// Returned when another process already holds the lock. The daemon should
/// print `.describe()` and exit non-zero rather than try to shadow the
/// existing instance.
#[derive(Debug)]
pub struct LockDenied {
    pub path: PathBuf,
    pub name: String,
}

impl LockDenied {
    /// Human-readable explanation for the operator, written to stderr.
    pub fn describe(&self) -> String {
        format!(
            "another '{name}' is already running against this database (lock: {path}). \
             Exactly one collector and one server may be running per database at a \
             time, but any number of frontend browser tabs may connect to the \
             server. Kill the existing one (e.g. `pkill -x spark-collect` or \
             `pkill -x spark-serve`, whichever holds the '{name}' lock) and try \
             again.",
            name = self.name,
            path = self.path.display()
        )
    }
}

impl SingleInstanceLock {
    /// Attempt to take the exclusive `flock` for the file `<db_dir>/.<name>.lock`,
    /// creating the parent directory if needed. Returns `Ok(lock)` on success —
    /// the returned handle MUST be kept alive for the whole lifetime of the
    /// daemon (it is usually declared near the top of `main()` so it outlives
    /// everything else).
    ///
    /// On Linux, this maps to `open(..., O_CREAT|O_RDWR)` followed by
    /// `flock(fd, LOCK_EX | LOCK_NB)`. `LOCK_NB` = non-blocking: we refuse to
    /// start immediately if another process holds the lock, rather than waiting
    /// (which would mask a real misconfiguration by eventually succeeding on its
    /// own).
    pub fn acquire(db_dir: &Path, name: &str) -> Result<Self, LockDenied> {
        let path = db_dir.join(format!(".{name}.lock"));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| LockDenied {
                path: path.clone(),
                name: name.to_string(),
            })?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|_| LockDenied { path: path.clone(), name: name.to_string() })?;

        // SAFETY: `file.as_fd()` is a valid, exclusively-owned open descriptor.
        // `flock(2)` is a synchronous, non-blocking syscall (we're passing
        // LOCK_NB) that operates only on that fd; it does not touch any other
        // memory and returns a simple integer status.
        let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if rc != 0 {
            // LOCK_NB guarantees we only get here if another process holds the
            // lock (EWOULDBLOCK) — we refuse to start and let the operator
            // clean up.
            return Err(LockDenied {
                path,
                name: name.to_string(),
            });
        }
        Ok(Self {
            _file: file,
        })
    }
}
