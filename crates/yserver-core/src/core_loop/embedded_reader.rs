//! Portable byte-stream reader; retains upstream request credits and BigRequests barrier.
use std::io;

use crossbeam_channel::{Receiver, TryRecvError};
use log::warn;

use yserver_protocol::x11::{self, ClientByteOrder, ClientId, SequenceNumber};

use crate::{
    core_loop::{message::Message, sender::BoundSender},
    server::ReaderControl,
    transport::Transport,
};

/// X11 BigRequests `Enable` minor opcode (always 0 — see X.Org spec).
pub const BIG_REQUESTS_ENABLE_MINOR: u8 = 0;

/// Maximum ordinary request bytes one client may have accepted but not yet
/// dispatched. This matches Xorg's 16 KiB initial per-client input buffer
/// (`os/io.c::BUFSIZE`). A single request larger than the remaining budget is
/// still framed, like Xorg growing its buffer for one complete large request;
/// the reader then stops until that request's bytes are returned.
pub const MAX_IN_FLIGHT_BYTES_PER_CLIENT: usize = 16 * 1024;

#[derive(Debug, Default)]
struct ReaderIngressWindow {
    outstanding_bytes: usize,
}

impl ReaderIngressWindow {
    fn can_read(&self) -> bool {
        self.outstanding_bytes < MAX_IN_FLIGHT_BYTES_PER_CLIENT
    }

    fn accept(&mut self, bytes: usize) {
        // One complete request may cross the window boundary, just as Xorg
        // grows its input buffer to hold one request. Preserve that overshoot
        // as outstanding debt; discarding it recreates unbounded read-ahead
        // when variable-sized requests repeat in a shifted cycle.
        self.outstanding_bytes = self.outstanding_bytes.saturating_add(bytes);
    }

    fn complete(&mut self, bytes: usize) {
        self.outstanding_bytes = self.outstanding_bytes.saturating_sub(bytes);
    }
}

/// Spawn the reader thread. Returns immediately; the thread runs
/// until it observes EOF, an unrecoverable framing error, or
/// `ReaderControl::Shutdown`.
///
/// `sender` must carry the generation this *connection* was accepted in
/// — the setup thread's binding, forwarded through
/// `Message::ClientSetupComplete`. A reader thread can still be parked
/// in `read` when a reset retires its session, and both messages it
/// produces (`Request`, `ClientDisconnected`) are session-scoped: tagged
/// with the generation running at send time they would be applied to the
/// *next* session instead of discarded.
pub fn spawn(
    id: ClientId,
    stream: impl Into<Transport>,
    byte_order: ClientByteOrder,
    big_requests_major: u8,
    control_rx: Receiver<ReaderControl>,
    sender: BoundSender,
) -> io::Result<()> {
    let stream = stream.into();
    std::thread::Builder::new()
        .name(format!("yserver-reader-{}", id.0))
        .spawn(move || {
            if let Err(e) = run(
                id,
                stream,
                byte_order,
                big_requests_major,
                control_rx,
                &sender,
            ) {
                let _ = sender.send(Message::ClientDisconnected { id, reason: e });
            }
        })?;
    Ok(())
}

fn run(
    id: ClientId,
    stream: Transport,
    byte_order: ClientByteOrder,
    big_requests_major: u8,
    control_rx: Receiver<ReaderControl>,
    sender: &BoundSender,
) -> io::Result<()> {
    let mut reader = stream;
    let mut big = false;
    let mut sequence: u16 = 0;
    let mut ingress = ReaderIngressWindow::default();
    let telemetry_enabled = std::env::var_os("YSERVER_LOOP_TELEMETRY").is_some();

    loop {
        // Consume returned credits and observe Shutdown before reading again.
        // If the credit window is exhausted, blocking here deliberately leaves
        // subsequent bytes in the client socket so the producer is throttled.
        loop {
            let control = if !ingress.can_read() {
                match control_rx.recv() {
                    Ok(control) => control,
                    Err(_) => return Ok(()),
                }
            } else {
                match control_rx.try_recv() {
                    Ok(control) => control,
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return Ok(()),
                }
            };
            match control {
                ReaderControl::Shutdown => return Ok(()),
                ReaderControl::GrantRequestBytes(bytes) => {
                    ingress.complete(bytes);
                }
                ReaderControl::ApplyBigRequests | ReaderControl::IgnoreBigRequests => {
                    // Apply/Ignore is valid only inside the barrier below.
                    warn!(
                        "client {}: stray ReaderControl outside BigRequests barrier",
                        id.0
                    );
                }
            }
        }

        let frame = match x11::read_request(&mut reader, byte_order, big) {
            Ok(Some(pair)) => pair,
            Ok(None) => {
                // Peer EOF — surface it to the core so
                // `process_disconnect` runs. The spawn-side `Err`
                // wrapper only fires on `io::Error`, not on Ok(None);
                // without this an Ok(None) leaks the client entry.
                let _ = sender.send(Message::ClientDisconnected {
                    id,
                    reason: io::Error::from(io::ErrorKind::UnexpectedEof),
                });
                return Ok(());
            }
            Err(e) => return Err(e),
        };
        let (header, mut body) = frame;
        let request_bytes = usize::try_from(header.length_units)
            .unwrap_or(usize::MAX)
            .saturating_mul(4)
            .max(4);
        // Phase E: swap inbound BE-client request bodies in-place so the
        // rest of the dispatch path can decode bytes as little-endian.
        x11::request_swap::swap_request_body(header.opcode, header.data, byte_order, &mut body);
        sequence = sequence.wrapping_add(1);
        let is_enable =
            header.opcode == big_requests_major && header.data == BIG_REQUESTS_ENABLE_MINOR;

        ingress.accept(request_bytes);
        sender.send(Message::Request {
            id,
            sequence: SequenceNumber(sequence),
            accepted_at: telemetry_enabled.then(std::time::Instant::now),
            header,
            body,
        })?;

        if is_enable {
            // Park until the core processes the Enable.
            loop {
                match control_rx.recv() {
                    Ok(ReaderControl::ApplyBigRequests) => {
                        big = true;
                        break;
                    }
                    Ok(ReaderControl::IgnoreBigRequests) => break,
                    Ok(ReaderControl::GrantRequestBytes(bytes)) => {
                        ingress.complete(bytes);
                    }
                    Ok(ReaderControl::Shutdown) | Err(_) => return Ok(()),
                }
            }
        }
    }
}
