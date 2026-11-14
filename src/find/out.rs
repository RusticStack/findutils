// Skua: standard output of `find`.
//
// Rust's `Stdout` flushes at every newline, and `-print` also flushed after every name, so
// listing a tree cost one `WriteFile` per entry. A terminal still wants a line at a time, but a
// pipe or a file (what an agent reads) is written in blocks, as GNU find's stdio does.
//
// The pending bytes are written, in this order, when the block is full, before any message on
// standard error (so the two streams interleave as they did when every line was written at
// once), before a child process is started (it shares the handle), and when `find` ends.

use std::cell::RefCell;
use std::io::{self, IsTerminal, Write};

/// Size of a block of output.
const BLOCK: usize = 64 * 1024;

thread_local! {
    static PENDING: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    /// The first write error met while flushing outside a `write` call (before a message on
    /// standard error, before a child): reported once by `finish`.
    static DEFERRED: RefCell<Option<io::Error>> = const { RefCell::new(None) };
}

/// `find`'s standard output: line buffered on a terminal, block buffered otherwise.
pub struct Stdout {
    block_buffered: bool,
}

impl Stdout {
    #[must_use]
    pub fn new() -> Self {
        Self {
            block_buffered: !io::stdout().is_terminal(),
        }
    }
}

impl Default for Stdout {
    fn default() -> Self {
        Self::new()
    }
}

impl Write for Stdout {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if !self.block_buffered {
            return io::stdout().write(buf);
        }
        self.write_all(buf)?;
        Ok(buf.len())
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        if !self.block_buffered {
            return io::stdout().write_all(buf);
        }
        let full = PENDING.with_borrow_mut(|pending| {
            pending.extend_from_slice(buf);
            pending.len() >= BLOCK
        });
        if full {
            write_pending()
        } else {
            Ok(())
        }
    }

    /// A flush after each name keeps a terminal current. Block-buffered output is written when
    /// the block is full or at the points listed above, not after every name.
    fn flush(&mut self) -> io::Result<()> {
        if self.block_buffered {
            Ok(())
        } else {
            io::stdout().flush()
        }
    }
}

fn write_pending() -> io::Result<()> {
    let bytes = PENDING.with_borrow_mut(std::mem::take);
    if bytes.is_empty() {
        return Ok(());
    }
    let mut out = io::stdout().lock();
    let result = out.write_all(&bytes).and_then(|()| out.flush());
    // Hand the allocation back for the next block.
    PENDING.with_borrow_mut(|pending| {
        if pending.is_empty() {
            let mut bytes = bytes;
            bytes.clear();
            *pending = bytes;
        }
    });
    result
}

/// Writes the pending output now. A closed pipe ends `find` at once with status 141 (as bash
/// reports a SIGPIPE death, like GNU find under Git Bash), so no `-exec`, `-ok` or `-delete`
/// runs after the reader is gone. Any other error is kept for `finish`.
pub fn flush_pending() {
    if let Err(e) = write_pending() {
        if e.kind() == io::ErrorKind::BrokenPipe && cfg!(all(windows, not(test))) {
            std::process::exit(141);
        }
        DEFERRED.with_borrow_mut(|slot| {
            if slot.is_none() {
                *slot = Some(e);
            }
        });
    }
}

/// `stderr()` for `find`'s messages: writes the pending standard output first.
pub fn stderr() -> io::Stderr {
    flush_pending();
    io::stderr()
}

/// Writes the rest of the output and returns the error that stopped it, if any.
pub fn finish() -> io::Result<()> {
    let result = write_pending();
    let deferred = DEFERRED.with_borrow_mut(Option::take);
    match (deferred, result) {
        (Some(e), _) | (None, Err(e)) => Err(e),
        (None, Ok(())) => Ok(()),
    }
}

/// `eprintln!` that writes the pending standard output first.
macro_rules! eprintln_flushed {
    ($($arg:tt)*) => {{
        $crate::find::out::flush_pending();
        eprintln!($($arg)*)
    }};
}
pub(crate) use eprintln_flushed;
