//! Shared access to the on-chip SPI flash.
//!
//! `esp-storage`'s `FlashStorage` wraps the `FLASH` peripheral itself (an
//! exclusively-owned singleton, obtained once from `esp_hal::init`), so
//! only one `FlashStorage` may ever exist. A firmware crate's persisted
//! settings (via `sequential-storage`, see [`crate::config_store`]) and its
//! OTA writer (via `esp-hal-ota`) both need flash access at arbitrary,
//! independent times, so this module owns the single real `FlashStorage`
//! behind a critical-section-guarded cell and hands out cheap
//! [`SharedFlash`] handles that delegate to it.
//!
//! Reads and NOR writes go straight to the ROM's flash functions, through a
//! small static bounce buffer: `esp-storage`'s keep a 4 KiB sector buffer on
//! the stack, and their stack frame has room for it even when it isn't
//! needed. Every task's flash access (settings, credentials) would cost that
//! much of the one main stack. The read-modify-write of [`Storage::write`]
//! (the OTA writer) still goes through `esp-storage`: it needs a sector
//! buffer.

use core::cell::RefCell;
use core::sync::atomic::{AtomicBool, Ordering};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;
use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use embedded_storage::{ReadStorage, Storage};
use esp_storage::{FlashStorage, FlashStorageError};

static FLASH: Mutex<CriticalSectionRawMutex, RefCell<Option<FlashStorage<'static>>>> =
    Mutex::new(RefCell::new(None));

/// Word-aligned, as the ROM reads whole words into it. Only used inside the
/// [`FLASH`] lock.
const BOUNCE_WORDS: usize = 128;
static BOUNCE: Mutex<CriticalSectionRawMutex, RefCell<[u32; BOUNCE_WORDS]>> =
    Mutex::new(RefCell::new([0; BOUNCE_WORDS]));

/// Reads any range (unaligned offset and length are fine) through
/// [`BOUNCE`].
fn read_through_bounce(
    flash: &FlashStorage<'static>,
    offset: u32,
    bytes: &mut [u8],
) -> Result<(), FlashStorageError> {
    let end = offset as usize + bytes.len();
    if end > ReadNorFlash::capacity(flash) {
        return Err(FlashStorageError::OutOfBounds);
    }
    BOUNCE.lock(|bounce| {
        let mut bounce = bounce.borrow_mut();
        let mut offset = offset;
        let mut bytes = bytes;
        while !bytes.is_empty() {
            let skip = (offset % 4) as usize;
            let len = bytes.len().min(BOUNCE_WORDS * 4 - skip);
            let words = (skip + len).div_ceil(4);
            // SAFETY: in bounds (checked above; the last word may reach up to
            // three bytes further, still inside the flash, whose size is a
            // multiple of 4), into a word-aligned buffer of at least
            // `words` words.
            unsafe {
                esp_storage::ll::spiflash_read(
                    offset - skip as u32,
                    bounce.as_mut_ptr(),
                    (words * 4) as u32,
                )
            }
            .map_err(rom_error)?;
            let read: &[u8] = as_bytes(&bounce[..words]);
            bytes[..len].copy_from_slice(&read[skip..skip + len]);
            offset += len as u32;
            bytes = &mut bytes[len..];
        }
        Ok(())
    })
}

/// Whether the ROM's write protection has been lifted (once per boot, like
/// `esp-storage` does).
static UNLOCKED: AtomicBool = AtomicBool::new(false);

/// NOR-writes `bytes` (offset and length word-aligned) through [`BOUNCE`].
/// Like `esp-storage`'s `write_nor`, without the check for a second running
/// core: the firmware never starts the ESP32's second core.
fn write_through_bounce(
    flash: &FlashStorage<'static>,
    offset: u32,
    bytes: &[u8],
) -> Result<(), FlashStorageError> {
    if !offset.is_multiple_of(4) || !bytes.len().is_multiple_of(4) {
        return Err(FlashStorageError::NotAligned);
    }
    if offset as usize + bytes.len() > ReadNorFlash::capacity(flash) {
        return Err(FlashStorageError::OutOfBounds);
    }
    if !UNLOCKED.load(Ordering::Relaxed) {
        // SAFETY: lifts the flash chip's write protection; no other effect.
        unsafe { esp_storage::ll::spiflash_unlock() }.map_err(|_| FlashStorageError::CantUnlock)?;
        UNLOCKED.store(true, Ordering::Relaxed);
    }
    BOUNCE.lock(|bounce| {
        let mut bounce = bounce.borrow_mut();
        for (i, chunk) in bytes.chunks(BOUNCE_WORDS * 4).enumerate() {
            let words = chunk.len() / 4;
            for (word, quad) in bounce[..words].iter_mut().zip(chunk.chunks_exact(4)) {
                *word = u32::from_ne_bytes([quad[0], quad[1], quad[2], quad[3]]);
            }
            // SAFETY: in bounds and aligned (checked above), from a
            // word-aligned buffer of `words` words.
            unsafe {
                esp_storage::ll::spiflash_write(
                    offset + (i * BOUNCE_WORDS * 4) as u32,
                    bounce.as_ptr(),
                    chunk.len() as u32,
                )
            }
            .map_err(rom_error)?;
        }
        Ok(())
    })
}

fn rom_error(rc: i32) -> FlashStorageError {
    match rc {
        1 => FlashStorageError::IoError,
        2 => FlashStorageError::IoTimeout,
        rc => FlashStorageError::Other(rc),
    }
}

fn as_bytes(words: &[u32]) -> &[u8] {
    // SAFETY: u32 has no padding and any byte pattern is a valid u8.
    unsafe { core::slice::from_raw_parts(words.as_ptr().cast(), words.len() * 4) }
}

/// Takes ownership of the `FLASH` peripheral. Must be called exactly once,
/// before any [`SharedFlash`] is used.
pub fn init(flash: esp_hal::peripherals::FLASH<'static>) {
    FLASH.lock(|cell| {
        *cell.borrow_mut() = Some(FlashStorage::new(flash));
    });
}

/// A cheap, `Copy` handle to the single shared [`FlashStorage`]. Every
/// operation locks the underlying critical-section mutex for its duration —
/// fine here since flash access is inherently exclusive/blocking anyway.
#[derive(Clone, Copy)]
pub struct SharedFlash;

impl SharedFlash {
    fn with<R>(&self, f: impl FnOnce(&mut FlashStorage<'static>) -> R) -> R {
        FLASH.lock(|cell| {
            let mut guard = cell.borrow_mut();
            let flash = guard.as_mut().expect("flash::init not called");
            f(flash)
        })
    }
}

impl embedded_storage::nor_flash::ErrorType for SharedFlash {
    type Error = FlashStorageError;
}

impl ReadStorage for SharedFlash {
    type Error = FlashStorageError;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        self.with(|flash| read_through_bounce(flash, offset, bytes))
    }

    fn capacity(&self) -> usize {
        self.with(|flash| ReadStorage::capacity(flash))
    }
}

impl Storage for SharedFlash {
    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        self.with(|flash| Storage::write(flash, offset, bytes))
    }
}

impl ReadNorFlash for SharedFlash {
    const READ_SIZE: usize = FlashStorage::READ_SIZE;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        if !offset.is_multiple_of(Self::READ_SIZE as u32)
            || !bytes.len().is_multiple_of(Self::READ_SIZE)
        {
            return Err(FlashStorageError::NotAligned);
        }
        self.with(|flash| read_through_bounce(flash, offset, bytes))
    }

    fn capacity(&self) -> usize {
        self.with(|flash| ReadNorFlash::capacity(flash))
    }
}

impl NorFlash for SharedFlash {
    const WRITE_SIZE: usize = FlashStorage::WRITE_SIZE;
    const ERASE_SIZE: usize = FlashStorage::ERASE_SIZE;

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        self.with(|flash| write_through_bounce(flash, offset, bytes))
    }

    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        self.with(|flash| NorFlash::erase(flash, from, to))
    }
}

impl embedded_storage::nor_flash::MultiwriteNorFlash for SharedFlash {}
