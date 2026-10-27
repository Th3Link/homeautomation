//! MbedTLS for the web server and the EST client (ADR 0018): the one
//! library instance, its random numbers, the one-session-at-a-time slot,
//! and the glue between an MbedTLS session and picoserve.
//!
//! A TLS session takes ~40 KB of heap (16 KiB records in, 8 KiB out, the
//! handshake), so only one runs at a time: the web server's connections
//! and the EST client take turns through [`slot`]. Further connections are
//! accepted and wait (they take the slot only once the client has sent
//! something, so a browser's speculative connections don't hold it); a
//! keep-alive connection lets go after a second of idleness.

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

/// What the one TLS session at a time needs besides MbedTLS' own heap: the
/// HTTP request buffer and the buffer responses are collected in. Kept here
/// once instead of in every web task, since only the slot holder uses them.
pub struct Buffers {
    pub http: [u8; 1536],
    pub out: [u8; OUT_BUFFER],
}

/// How much of a response is collected before it goes out as one TLS
/// record (each record costs a header, a tag and an encryption pass).
pub const OUT_BUFFER: usize = 2048;

static SLOT: Mutex<CriticalSectionRawMutex, Buffers> = Mutex::new(Buffers {
    http: [0; 1536],
    out: [0; OUT_BUFFER],
});

/// The right to run a TLS session, with the buffers for it; held for the
/// session's life.
pub async fn slot() -> MutexGuard<'static, CriticalSectionRawMutex, Buffers> {
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
/// split MbedTLS session, the write half buffered. Closing the session and
/// the socket is the caller's business once picoserve is done.
pub struct TlsConnection<'b, R, W> {
    pub read: R,
    pub write: BufferedWriter<'b, W>,
}

impl<'b, R, W> TlsConnection<'b, R, W> {
    pub fn new(read: R, write: W, out: &'b mut [u8]) -> Self {
        Self {
            read,
            write: BufferedWriter {
                inner: write,
                buf: out,
                len: 0,
            },
        }
    }
}

/// Collects what picoserve writes (it writes a response in many small
/// pieces, a header line at a time) and hands it to MbedTLS in large
/// pieces, so a response goes out in a few TLS records instead of dozens.
/// picoserve flushes after every response.
pub struct BufferedWriter<'b, W> {
    inner: W,
    buf: &'b mut [u8],
    len: usize,
}

/// `write_with` drains the buffer first when less than this is free.
const MIN_ROOM: usize = 256;

impl<W: Write> BufferedWriter<'_, W> {
    async fn drain(&mut self) -> Result<(), W::Error> {
        if self.len > 0 {
            self.inner.write_all(&self.buf[..self.len]).await?;
            self.len = 0;
        }
        Ok(())
    }
}

impl<W: Write> ErrorType for BufferedWriter<'_, W> {
    type Error = W::Error;
}

impl<W: Write> Write for BufferedWriter<'_, W> {
    async fn write(&mut self, data: &[u8]) -> Result<usize, Self::Error> {
        if self.len + data.len() > self.buf.len() {
            self.drain().await?;
        }
        if data.len() >= self.buf.len() {
            return self.inner.write(data).await;
        }
        self.buf[self.len..self.len + data.len()].copy_from_slice(data);
        self.len += data.len();
        Ok(data.len())
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        self.drain().await?;
        self.inner.flush().await
    }
}

impl<W: Write> picoserve::io::Write for BufferedWriter<'_, W> {
    async fn write_with<F: FnOnce(picoserve::mem::BorrowedCursor<'_>) -> T, T>(
        &mut self,
        f: F,
    ) -> Result<T, Self::Error> {
        if self.buf.len() - self.len < MIN_ROOM {
            self.drain().await?;
        }
        let mut buffer = picoserve::mem::BorrowedBuffer::new(&mut self.buf[self.len..]);
        let output = f(buffer.unfilled());
        self.len += buffer.filled().len();
        Ok(output)
    }
}

impl<'b, R, W, E> picoserve::io::Socket<picoserve::EmbassyRuntime> for TlsConnection<'b, R, W>
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
        = &'a mut BufferedWriter<'b, W>
    where
        Self: 'a;

    fn split(&mut self) -> (Self::ReadHalf<'_>, Self::WriteHalf<'_>) {
        (&mut self.read, &mut self.write)
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
