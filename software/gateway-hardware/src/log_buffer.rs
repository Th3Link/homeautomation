//! The latest log lines, for the web UI: every
//! [`console_log!`](crate::console_log) line goes into a static ring of
//! [`SIZE`] bytes, the oldest overwritten. Readers ask by position in the
//! stream of everything ever logged, so they get each line once.

use core::cell::RefCell;
use core::fmt::Write;
use critical_section::Mutex;

/// Bytes of log kept.
pub const SIZE: usize = 6 * 1024;

struct Ring {
    buf: [u8; SIZE],
    /// Bytes logged since boot; `written % SIZE` is where the next goes.
    written: u32,
    /// Whether the next byte starts a line (it gets the uptime first).
    line_start: bool,
}

impl Ring {
    fn push(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.buf[self.written as usize % SIZE] = b;
            self.written = self.written.wrapping_add(1);
        }
    }
}

impl Write for Ring {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.push(s.as_bytes());
        Ok(())
    }
}

static RING: Mutex<RefCell<Ring>> = Mutex::new(RefCell::new(Ring {
    buf: [0; SIZE],
    written: 0,
    line_start: true,
}));

/// Starts keeping the log lines. Once, early in `main`.
pub fn install() {
    common_hardware::logging::set_sink(record);
}

fn record(line: core::fmt::Arguments<'_>) {
    let uptime_ms = embassy_time::Instant::now().as_millis();
    critical_section::with(|cs| {
        let mut ring = RING.borrow_ref_mut(cs);
        if ring.line_start {
            let _ = write!(
                ring,
                "[{:>6}.{}] ",
                uptime_ms / 1000,
                uptime_ms % 1000 / 100
            );
        }
        let _ = ring.write_fmt(line);
        ring.push(b"\n");
        ring.line_start = true;
    });
}

/// Up to `max` bytes of log from position `from` on (the oldest kept, if
/// `from` is older), whole lines only: the position of the first byte, the
/// text and the position after it.
pub fn read<const N: usize>(from: u32) -> (u32, heapless::String<N>, u32) {
    let mut bytes = heapless::Vec::new();
    let (start, end) = read_into(from, &mut bytes);
    (
        start,
        heapless::String::from_utf8(bytes).unwrap_or_default(),
        end,
    )
}

/// Copies whole lines into caller-owned storage, truncating a torn UTF-8 suffix.
pub fn read_into<const N: usize>(from: u32, bytes: &mut heapless::Vec<u8, N>) -> (u32, u32) {
    critical_section::with(|cs| {
        let ring = RING.borrow_ref(cs);
        let oldest = ring.written.saturating_sub(SIZE as u32);
        let mut start = from.clamp(oldest, ring.written);
        let at = |pos: u32| ring.buf[pos as usize % SIZE];
        // From the oldest kept: begin after the first line break (that
        // line's beginning is gone).
        if from < oldest {
            while start < ring.written && at(start) != b'\n' {
                start += 1;
            }
            start = (start + 1).min(ring.written);
        }
        // Whole lines that fit.
        let mut end = start;
        let mut pos = start;
        while pos < ring.written && (pos - start) < N as u32 {
            if at(pos) == b'\n' {
                end = pos + 1;
            }
            pos += 1;
        }
        bytes.clear();
        // Copy at most two contiguous spans. A UTF-8 character may straddle
        // the end of the ring, so validate the combined bytes.
        let offset = start as usize % SIZE;
        let len = (end - start) as usize;
        let first = len.min(SIZE - offset);
        bytes
            .extend_from_slice(&ring.buf[offset..offset + first])
            .unwrap();
        bytes.extend_from_slice(&ring.buf[..len - first]).unwrap();
        if let Err(e) = core::str::from_utf8(bytes) {
            bytes.truncate(e.valid_up_to());
        }
        (start, end)
    })
}
