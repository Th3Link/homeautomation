//! MbedTLS for the web server and the EST client (ADR 0018): the one
//! library instance, its random numbers, the one-session-at-a-time slot,
//! and the glue between an MbedTLS session and picoserve.
//!
//! A TLS session takes ~40 KB of heap (16 KiB records in, 8 KiB out, the
//! handshake), so only one runs at a time: the web server's connections
//! and the EST client take turns through [`slot`]. Further connections are
//! accepted and wait; a browser's keep-alive connection lets go after a
//! second of idleness.

use embassy_net::tcp::{TcpReader, TcpSocket, TcpWriter};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::{Mutex, MutexGuard};
use embassy_sync::once_lock::OnceLock;
use embedded_io_async::{ErrorType, Read, Write};
use esp_hal::rng::Trng;
use mbedtls_rs::{Tls, TlsReference};
use static_cell::StaticCell;

/// The hardware RNG (with the ADC entropy source on, see
/// [`crate::tls::init_entropy`]) for MbedTLS.
struct HardwareRng(Trng);

impl rand_core::TryRng for HardwareRng {
    type Error = core::convert::Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
        Ok(self.0.random())
    }

    fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
        Ok(u64::from(self.0.random()) << 32 | u64::from(self.0.random()))
    }

    fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), Self::Error> {
        self.0.read(dst);
        Ok(())
    }
}

impl rand_core::TryCryptoRng for HardwareRng {}

static RNG: StaticCell<HardwareRng> = StaticCell::new();
static TLS: StaticCell<Tls<'static>> = StaticCell::new();
static INSTANCE: OnceLock<&'static Tls<'static>> = OnceLock::new();

/// Starts MbedTLS. Once, after the entropy source is on.
pub fn init() {
    let Ok(trng) = Trng::try_new() else {
        crate::console_log!("https: no hardware RNG, TLS disabled");
        return;
    };
    crate::wall_clock::hook_mbedtls();
    let rng = RNG.init(HardwareRng(trng));
    match Tls::new(rng) {
        Ok(tls) => {
            let _ = INSTANCE.init(TLS.init(tls));
        }
        Err(_) => crate::console_log!("https: MbedTLS already started"),
    }
}

/// The MbedTLS instance, once [`init`] has run.
pub fn tls() -> Option<TlsReference<'static>> {
    INSTANCE.try_get().map(|tls| tls.reference())
}

static SLOT: Mutex<CriticalSectionRawMutex, ()> = Mutex::new(());

/// The right to run a TLS session; held for the session's life.
pub async fn slot() -> MutexGuard<'static, CriticalSectionRawMutex, ()> {
    SLOT.lock().await
}

/// A TCP socket that MbedTLS can split into a read and a write half.
pub struct TcpStream<'a, 's>(pub &'a mut TcpSocket<'s>);

impl ErrorType for TcpStream<'_, '_> {
    type Error = embassy_net::tcp::Error;
}

impl Read for TcpStream<'_, '_> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.0.read(buf).await
    }
}

impl Write for TcpStream<'_, '_> {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.0.write(buf).await
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        self.0.flush().await
    }
}

impl mbedtls_rs::Split for TcpStream<'_, '_> {
    type Read<'a>
        = TcpReader<'a>
    where
        Self: 'a;
    type Write<'a>
        = TcpWriter<'a>
    where
        Self: 'a;

    fn split(&mut self) -> (Self::Read<'_>, Self::Write<'_>) {
        self.0.split()
    }
}

/// An open TLS session as picoserve sees a connection: the two halves of a
/// split MbedTLS session. Closing the session and the socket is the
/// caller's business once picoserve is done.
pub struct TlsConnection<R, W> {
    pub read: R,
    pub write: W,
}

/// picoserve's writer has to lend a buffer; MbedTLS' doesn't have one to
/// lend, so this one stands in.
pub struct LendingWriter<'a, W> {
    inner: &'a mut W,
}

impl<W: Write> ErrorType for LendingWriter<'_, W> {
    type Error = W::Error;
}

impl<W: Write> Write for LendingWriter<'_, W> {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.inner.write(buf).await
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        self.inner.flush().await
    }
}

impl<W: Write> picoserve::io::Write for LendingWriter<'_, W> {
    async fn write_with<F: FnOnce(picoserve::mem::BorrowedCursor<'_>) -> T, T>(
        &mut self,
        f: F,
    ) -> Result<T, Self::Error> {
        let mut buffer = [0u8; 512];
        let mut buffer = picoserve::mem::BorrowedBuffer::new(&mut buffer);
        let output = f(buffer.unfilled());
        self.inner.write_all(buffer.filled()).await?;
        Ok(output)
    }
}

impl<R, W, E> picoserve::io::Socket<picoserve::EmbassyRuntime> for TlsConnection<R, W>
where
    R: Read<Error = E>,
    W: Write<Error = E>,
    E: embedded_io_async::Error + 'static,
{
    type Error = E;
    type ReadHalf<'a>
        = &'a mut R
    where
        Self: 'a;
    type WriteHalf<'a>
        = LendingWriter<'a, W>
    where
        Self: 'a;

    fn split(&mut self) -> (Self::ReadHalf<'_>, Self::WriteHalf<'_>) {
        (
            &mut self.read,
            LendingWriter {
                inner: &mut self.write,
            },
        )
    }

    async fn abort<T: picoserve::Timer<picoserve::EmbassyRuntime>>(
        self,
        _timeouts: &picoserve::Timeouts,
        _timer: &T,
    ) -> Result<(), picoserve::Error<Self::Error>> {
        Ok(())
    }

    async fn shutdown<T: picoserve::Timer<picoserve::EmbassyRuntime>>(
        mut self,
        timeouts: &picoserve::Timeouts,
        timer: &T,
    ) -> Result<(), picoserve::Error<Self::Error>> {
        timer
            .run_with_timeout(timeouts.write, self.write.flush())
            .await
            .map_err(picoserve::Error::WriteTimeout)?
            .map_err(picoserve::Error::Write)
    }
}
