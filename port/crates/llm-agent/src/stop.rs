// SPDX-License-Identifier: GPL-3.0-or-later

//! Stopping a model request while it waits for the server.
//!
//! A model's answer can take a minute, so the transport can't simply time
//! out sooner. Instead, [`StoppableStream`] reads with a short socket
//! timeout and asks a [`StopCheck`] between attempts, so a stopped request
//! (the user's Stop, or the plugin closing) returns within [`POLL`] instead
//! of waiting out the server. The overall limit becomes an idle limit: no
//! bytes for [`IDLE`].

use std::fmt;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Asked while a request waits; true stops it.
pub type StopCheck = Arc<dyn Fn() -> bool + Send + Sync>;

/// How often a waiting read asks its [`StopCheck`].
pub const POLL: Duration = Duration::from_millis(250);
/// How long a read may wait for the server's next bytes.
pub const IDLE: Duration = Duration::from_secs(120);

/// The error a stopped read returns.
pub const STOPPED: &str = "the request was stopped";

/// A stream whose reads can be stopped; see the module docs.
pub struct StoppableStream<S> {
    inner: S,
    /// The connection's socket (a clone of the handle), for its timeout.
    socket: Option<TcpStream>,
    stop: StopCheck,
    idle: Duration,
}

impl<S> StoppableStream<S> {
    /// `inner`, whose bytes travel over `socket`.
    pub fn new(inner: S, socket: Option<&TcpStream>, stop: StopCheck) -> StoppableStream<S> {
        StoppableStream {
            inner,
            socket: socket.and_then(|socket| socket.try_clone().ok()),
            stop,
            idle: IDLE,
        }
    }

    /// The idle limit, in place of [`IDLE`].
    pub fn with_idle(mut self, idle: Duration) -> StoppableStream<S> {
        self.idle = idle;
        self
    }

    pub fn socket(&self) -> Option<&TcpStream> {
        self.socket.as_ref()
    }
}

impl<S: Read> Read for StoppableStream<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if (self.stop)() {
            return Err(io::Error::other(STOPPED));
        }
        let Some(socket) = &self.socket else {
            return self.inner.read(buf);
        };
        socket.set_read_timeout(Some(POLL))?;
        let started = Instant::now();
        loop {
            match self.inner.read(buf) {
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    if (self.stop)() {
                        return Err(io::Error::other(STOPPED));
                    }
                    if started.elapsed() >= self.idle {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "the server sent nothing for too long",
                        ));
                    }
                }
                result => return result,
            }
        }
    }
}

impl<S: Write> Write for StoppableStream<S> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<S> fmt::Debug for StoppableStream<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StoppableStream")
            .field("socket", &self.socket)
            .finish_non_exhaustive()
    }
}

/// HTTPS connections through [`StoppableStream`], for `ureq`.
#[cfg(feature = "http")]
pub(crate) mod tls {
    use super::*;

    impl ureq::ReadWrite for StoppableStream<Box<dyn ureq::ReadWrite>> {
        fn socket(&self) -> Option<&TcpStream> {
            StoppableStream::socket(self)
        }
    }

    /// The system's TLS over a stoppable TCP stream.
    pub(crate) struct StoppableTls {
        pub(crate) inner: Arc<native_tls::TlsConnector>,
        pub(crate) stop: StopCheck,
    }

    impl ureq::TlsConnector for StoppableTls {
        fn connect(
            &self,
            dns_name: &str,
            io: Box<dyn ureq::ReadWrite>,
        ) -> Result<Box<dyn ureq::ReadWrite>, ureq::Error> {
            let socket = io.socket().and_then(|socket| socket.try_clone().ok());
            let stream = StoppableStream::new(io, socket.as_ref(), self.stop.clone());
            ureq::TlsConnector::connect(&*self.inner, dns_name, Box::new(stream))
        }
    }
}
