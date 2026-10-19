//! Socket transport abstractions for X11 clients.
//!
//! Unix-domain clients are local and can pass descriptors. Opt-in TCP clients
//! use the same byte-stream setup/read/write path with remote-client policy.

use std::{
    io::{self, Read, Write},
    net::Shutdown,
    time::Duration,
};

#[cfg(feature = "embedded")]
use crate::MemoryStream;
#[cfg(feature = "native")]
use std::{
    net::{TcpListener, TcpStream},
    os::{
        fd::{AsRawFd, RawFd},
        unix::net::{UnixListener, UnixStream},
    },
};

#[cfg(all(test, feature = "native"))]
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

#[derive(Debug)]
pub enum Transport {
    #[cfg(feature = "native")]
    Unix(UnixStream),
    #[cfg(feature = "native")]
    Tcp(TcpStream),
    #[cfg(feature = "embedded")]
    Memory(MemoryStream),
    #[cfg(all(test, feature = "native"))]
    Capture(Arc<Mutex<VecDeque<u8>>>),
}

#[cfg(all(test, feature = "native"))]
pub(crate) struct CapturedPeer {
    bytes: Arc<Mutex<VecDeque<u8>>>,
    nonblocking: bool,
}

#[cfg(all(test, feature = "native"))]
impl CapturedPeer {
    pub(crate) fn set_nonblocking(&mut self, nonblocking: bool) -> io::Result<()> {
        self.nonblocking = nonblocking;
        Ok(())
    }
}

#[cfg(all(test, feature = "native"))]
impl Read for CapturedPeer {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut bytes = self.bytes.lock().unwrap();
        if bytes.is_empty() {
            return if self.nonblocking {
                Err(io::ErrorKind::WouldBlock.into())
            } else {
                Ok(0)
            };
        }
        let count = buf.len().min(bytes.len());
        for slot in &mut buf[..count] {
            *slot = bytes.pop_front().expect("count is within capture length");
        }
        Ok(count)
    }
}

impl Transport {
    pub fn is_memory(&self) -> bool {
        #[cfg(feature = "embedded")]
        if matches!(self, Self::Memory(_)) {
            return true;
        }
        false
    }
    pub(crate) fn set_write_waker(&self, sender: crate::core_loop::CoreSender) {
        match self {
            #[cfg(feature = "embedded")]
            Self::Memory(stream) => stream.set_write_waker(sender),
            #[cfg(feature = "native")]
            _ => {
                let _ = sender;
            }
        }
    }

    #[cfg(all(test, feature = "native"))]
    pub(crate) fn capture_pair() -> (Self, CapturedPeer) {
        let bytes = Arc::new(Mutex::new(VecDeque::new()));
        (
            Self::Capture(bytes.clone()),
            CapturedPeer {
                bytes,
                nonblocking: false,
            },
        )
    }

    #[cfg(feature = "native")]
    pub fn pair() -> io::Result<(Self, UnixStream)> {
        UnixStream::pair().map(|(stream, peer)| (Self::Unix(stream), peer))
    }

    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        match self {
            #[cfg(feature = "native")]
            Self::Unix(stream) => stream.set_nonblocking(nonblocking),
            #[cfg(feature = "native")]
            Self::Tcp(stream) => stream.set_nonblocking(nonblocking),
            #[cfg(feature = "embedded")]
            Self::Memory(stream) => stream.set_nonblocking(nonblocking),
            #[cfg(all(test, feature = "native"))]
            Self::Capture(_) => Ok(()),
        }
    }

    pub fn try_clone(&self) -> io::Result<Self> {
        match self {
            #[cfg(feature = "native")]
            Self::Unix(stream) => stream.try_clone().map(Self::Unix),
            #[cfg(feature = "native")]
            Self::Tcp(stream) => stream.try_clone().map(Self::Tcp),
            #[cfg(feature = "embedded")]
            Self::Memory(stream) => stream.try_clone().map(Self::Memory),
            #[cfg(all(test, feature = "native"))]
            Self::Capture(bytes) => Ok(Self::Capture(bytes.clone())),
        }
    }

    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        match self {
            #[cfg(feature = "native")]
            Self::Unix(stream) => stream.shutdown(how),
            #[cfg(feature = "native")]
            Self::Tcp(stream) => stream.shutdown(how),
            #[cfg(feature = "embedded")]
            Self::Memory(stream) => stream.shutdown(how),
            #[cfg(all(test, feature = "native"))]
            Self::Capture(_) => Ok(()),
        }
    }

    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        match self {
            #[cfg(feature = "native")]
            Self::Unix(stream) => stream.set_read_timeout(timeout),
            #[cfg(feature = "native")]
            Self::Tcp(stream) => stream.set_read_timeout(timeout),
            #[cfg(feature = "embedded")]
            Self::Memory(stream) => stream.set_read_timeout(timeout),
            #[cfg(all(test, feature = "native"))]
            Self::Capture(_) => Ok(()),
        }
    }

    pub fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        match self {
            #[cfg(feature = "native")]
            Self::Unix(stream) => stream.set_write_timeout(timeout),
            #[cfg(feature = "native")]
            Self::Tcp(stream) => stream.set_write_timeout(timeout),
            #[cfg(feature = "embedded")]
            Self::Memory(stream) => stream.set_write_timeout(timeout),
            #[cfg(all(test, feature = "native"))]
            Self::Capture(_) => Ok(()),
        }
    }
}

#[cfg(feature = "native")]
impl From<UnixStream> for Transport {
    fn from(stream: UnixStream) -> Self {
        #[cfg(feature = "native")]
        Self::Unix(stream)
    }
}

impl Read for Transport {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            #[cfg(feature = "native")]
            Self::Unix(stream) => stream.read(buf),
            #[cfg(feature = "native")]
            Self::Tcp(stream) => stream.read(buf),
            #[cfg(feature = "embedded")]
            Self::Memory(stream) => stream.read(buf),
            #[cfg(all(test, feature = "native"))]
            Self::Capture(_) => Ok(0),
        }
    }
}

impl Write for Transport {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            #[cfg(feature = "native")]
            Self::Unix(stream) => stream.write(buf),
            #[cfg(feature = "native")]
            Self::Tcp(stream) => stream.write(buf),
            #[cfg(feature = "embedded")]
            Self::Memory(stream) => stream.write(buf),
            #[cfg(all(test, feature = "native"))]
            Self::Capture(bytes) => {
                bytes.lock().unwrap().extend(buf.iter().copied());
                Ok(buf.len())
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            #[cfg(feature = "native")]
            Self::Unix(stream) => stream.flush(),
            #[cfg(feature = "native")]
            Self::Tcp(stream) => stream.flush(),
            #[cfg(feature = "embedded")]
            Self::Memory(stream) => stream.flush(),
            #[cfg(all(test, feature = "native"))]
            Self::Capture(_) => Ok(()),
        }
    }
}

#[cfg(feature = "native")]
impl AsRawFd for Transport {
    fn as_raw_fd(&self) -> RawFd {
        match self {
            #[cfg(feature = "native")]
            Self::Unix(stream) => stream.as_raw_fd(),
            #[cfg(feature = "native")]
            Self::Tcp(stream) => stream.as_raw_fd(),
            #[cfg(feature = "embedded")]
            Self::Memory(_) => -1,
            #[cfg(all(test, feature = "native"))]
            Self::Capture(_) => -1,
        }
    }
}

pub enum Listener {
    #[cfg(feature = "native")]
    Unix(UnixListener),
    #[cfg(feature = "native")]
    Tcp(TcpListener),
}

#[cfg(feature = "native")]
impl From<UnixListener> for Listener {
    fn from(listener: UnixListener) -> Self {
        #[cfg(feature = "native")]
        Self::Unix(listener)
    }
}

#[cfg(feature = "native")]
impl Listener {
    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        match self {
            #[cfg(feature = "native")]
            Self::Unix(listener) => listener.set_nonblocking(nonblocking),
            #[cfg(feature = "native")]
            Self::Tcp(listener) => listener.set_nonblocking(nonblocking),
        }
    }
}

#[cfg(feature = "native")]
impl AsRawFd for Listener {
    fn as_raw_fd(&self) -> RawFd {
        match self {
            #[cfg(feature = "native")]
            Self::Unix(listener) => listener.as_raw_fd(),
            #[cfg(feature = "native")]
            Self::Tcp(listener) => listener.as_raw_fd(),
        }
    }
}

#[cfg(feature = "embedded")]
impl From<MemoryStream> for Transport {
    fn from(stream: MemoryStream) -> Self {
        Self::Memory(stream)
    }
}
