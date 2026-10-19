//! Bounded, in-process duplex byte streams. No sockets, descriptors or listeners.
//! Clones share lifetime/shutdown, with independent blocking modes and timeouts.
use crate::core_loop::{CoreSender, Message};
use std::{
    collections::VecDeque,
    io::{self, Read, Write},
    net::Shutdown,
    sync::{Arc, Condvar, Mutex, MutexGuard},
    time::{Duration, Instant},
};
const CAPACITY: usize = 256 * 1024;
#[derive(Default)]
struct Buffer {
    bytes: VecDeque<u8>,
    read_closed: bool,
    write_closed: bool,
    write_waker: Option<CoreSender>,
}
#[derive(Default)]
struct Pipe {
    buffer: Mutex<Buffer>,
    changed: Condvar,
}
struct Endpoint {
    incoming: Arc<Pipe>,
    outgoing: Arc<Pipe>,
}
impl Endpoint {
    fn shutdown(&self, how: Shutdown) {
        if matches!(how, Shutdown::Read | Shutdown::Both) {
            let mut buffer = self.incoming.buffer.lock().unwrap();
            buffer.read_closed = true;
            buffer.bytes.clear();
            let waker = buffer.write_waker.clone();
            drop(buffer);
            self.incoming.changed.notify_all();
            if let Some(waker) = waker {
                let _ = waker.send(Message::PlatformEvent);
            }
        }
        if matches!(how, Shutdown::Write | Shutdown::Both) {
            self.outgoing.buffer.lock().unwrap().write_closed = true;
            self.outgoing.changed.notify_all();
        }
    }
}
impl Drop for Endpoint {
    fn drop(&mut self) {
        self.shutdown(Shutdown::Both);
    }
}
/// A bounded duplex byte stream. Dropping the last clone closes its endpoint.
#[derive(Clone, Copy, Default)]
struct Settings {
    nonblocking: bool,
    read_timeout: Option<Duration>,
    write_timeout: Option<Duration>,
}
pub struct MemoryStream {
    endpoint: Arc<Endpoint>,
    settings: Mutex<Settings>,
}
impl Clone for MemoryStream {
    fn clone(&self) -> Self {
        Self {
            endpoint: self.endpoint.clone(),
            settings: Mutex::new(*self.settings.lock().unwrap()),
        }
    }
}
impl std::fmt::Debug for MemoryStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stream")
            .field("nonblocking", &self.settings.lock().unwrap().nonblocking)
            .finish_non_exhaustive()
    }
}
impl MemoryStream {
    pub fn pair() -> io::Result<(Self, Self)> {
        let a = Arc::new(Pipe::default());
        let b = Arc::new(Pipe::default());
        let make = |incoming, outgoing| Self {
            endpoint: Arc::new(Endpoint { incoming, outgoing }),
            settings: Mutex::new(Settings::default()),
        };
        Ok((make(a.clone(), b.clone()), make(b, a)))
    }
    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(self.clone())
    }
    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.endpoint.shutdown(how);
        Ok(())
    }
    pub fn set_nonblocking(&self, value: bool) -> io::Result<()> {
        self.settings.lock().unwrap().nonblocking = value;
        Ok(())
    }
    pub fn set_read_timeout(&self, value: Option<Duration>) -> io::Result<()> {
        validate_timeout(value)?;
        self.settings.lock().unwrap().read_timeout = value;
        Ok(())
    }
    pub fn set_write_timeout(&self, value: Option<Duration>) -> io::Result<()> {
        validate_timeout(value)?;
        self.settings.lock().unwrap().write_timeout = value;
        Ok(())
    }
    pub(crate) fn set_write_waker(&self, sender: CoreSender) {
        self.endpoint.outgoing.buffer.lock().unwrap().write_waker = Some(sender);
    }
}
fn validate_timeout(timeout: Option<Duration>) -> io::Result<()> {
    if timeout == Some(Duration::ZERO) {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "zero stream timeout",
        ))
    } else {
        Ok(())
    }
}
fn wait<'a>(
    pipe: &'a Pipe,
    buffer: MutexGuard<'a, Buffer>,
    deadline: Option<Instant>,
) -> io::Result<MutexGuard<'a, Buffer>> {
    if let Some(deadline) = deadline {
        let duration = deadline.saturating_duration_since(Instant::now());
        if duration.is_zero() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        let (buffer, _) = pipe.changed.wait_timeout(buffer, duration).unwrap();
        Ok(buffer)
    } else {
        Ok(pipe.changed.wait(buffer).unwrap())
    }
}
impl Read for MemoryStream {
    fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        if dst.is_empty() {
            return Ok(0);
        }
        let pipe = &self.endpoint.incoming;
        let settings = *self.settings.lock().unwrap();
        let deadline = settings.read_timeout.map(|d| Instant::now() + d);
        let mut buffer = pipe.buffer.lock().unwrap();
        loop {
            if buffer.read_closed {
                return Ok(0);
            }
            if !buffer.bytes.is_empty() {
                let count = dst.len().min(buffer.bytes.len());
                for slot in &mut dst[..count] {
                    *slot = buffer.bytes.pop_front().unwrap();
                }
                let waker = buffer.write_waker.clone();
                drop(buffer);
                pipe.changed.notify_all();
                if let Some(waker) = waker {
                    let _ = waker.send(Message::PlatformEvent);
                }
                return Ok(count);
            }
            if buffer.write_closed {
                return Ok(0);
            }
            if settings.nonblocking {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            buffer = wait(pipe, buffer, deadline)?;
        }
    }
}
impl Write for MemoryStream {
    fn write(&mut self, src: &[u8]) -> io::Result<usize> {
        if src.is_empty() {
            return Ok(0);
        }
        let pipe = &self.endpoint.outgoing;
        let settings = *self.settings.lock().unwrap();
        let deadline = settings.write_timeout.map(|d| Instant::now() + d);
        let mut buffer = pipe.buffer.lock().unwrap();
        loop {
            if buffer.write_closed || buffer.read_closed {
                return Err(io::ErrorKind::BrokenPipe.into());
            }
            let count = src.len().min(CAPACITY - buffer.bytes.len());
            if count > 0 {
                buffer.bytes.extend(&src[..count]);
                drop(buffer);
                pipe.changed.notify_all();
                return Ok(count);
            }
            if settings.nonblocking {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            buffer = wait(pipe, buffer, deadline)?;
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
