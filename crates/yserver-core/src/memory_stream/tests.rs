use super::*;
use std::{sync::mpsc, thread};

#[test]
fn bounded_writes_resume_after_peer_reads_and_wake_core() {
    let (mut writer, mut reader) = MemoryStream::pair().unwrap();
    let (_poll, sender, receiver) = crate::core_loop::sender::channel().unwrap();
    writer.set_write_waker(sender);
    writer.set_nonblocking(true).unwrap();
    assert_eq!(writer.write(&vec![7; CAPACITY + 20]).unwrap(), CAPACITY);
    assert_eq!(
        writer.write(&[8]).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    let mut first = [0; 20];
    reader.read_exact(&mut first).unwrap();
    assert_eq!(first, [7; 20]);
    assert!(matches!(
        receiver.try_recv_all().next(),
        Some(Message::PlatformEvent)
    ));
    assert_eq!(writer.write(&[8; 20]).unwrap(), 20);
    writer.shutdown(Shutdown::Write).unwrap();
    let mut tail = Vec::new();
    reader.read_to_end(&mut tail).unwrap();
    assert_eq!(tail.len(), CAPACITY);
    assert!(tail[..CAPACITY - 20].iter().all(|b| *b == 7));
    assert_eq!(&tail[CAPACITY - 20..], &[8; 20]);
}

#[test]
fn clone_blocking_mode_is_independent_and_last_drop_closes() {
    let (mut a, mut b) = MemoryStream::pair().unwrap();
    let mut clone = a.try_clone().unwrap();
    clone.set_nonblocking(true).unwrap();
    assert_eq!(
        clone.read(&mut [0]).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    let (tx, rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        let mut byte = [0];
        a.read_exact(&mut byte).unwrap();
        tx.send(byte).unwrap();
    });
    b.write_all(&[42]).unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), [42]);
    worker.join().unwrap();
    b.set_nonblocking(true).unwrap();
    assert_eq!(
        b.read(&mut [0]).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    drop(clone);
    assert_eq!(b.read(&mut [0]).unwrap(), 0);
    assert_eq!(b.write(&[1]).unwrap_err().kind(), io::ErrorKind::BrokenPipe);
}

#[test]
fn shutdown_unblocks_readers_and_writers() {
    let (mut a, b) = MemoryStream::pair().unwrap();
    let close_a = a.try_clone().unwrap();
    let (tx, rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        tx.send(a.read(&mut [0])).unwrap();
    });
    close_a.shutdown(Shutdown::Both).unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap().unwrap(), 0);
    reader.join().unwrap();
    drop(b);

    let (mut a, b) = MemoryStream::pair().unwrap();
    a.write_all(&vec![1; CAPACITY]).unwrap();
    let (tx, rx) = mpsc::channel();
    let writer = thread::spawn(move || {
        tx.send(a.write(&[2])).unwrap();
    });
    b.shutdown(Shutdown::Read).unwrap();
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap_err()
            .kind(),
        io::ErrorKind::BrokenPipe
    );
    writer.join().unwrap();
}

#[test]
fn half_close_keeps_reverse_direction_alive() {
    let (mut a, mut b) = MemoryStream::pair().unwrap();
    a.write_all(b"hello").unwrap();
    a.shutdown(Shutdown::Write).unwrap();
    let mut data = Vec::new();
    b.read_to_end(&mut data).unwrap();
    assert_eq!(data, b"hello");
    b.write_all(b"ok").unwrap();
    let mut response = [0; 2];
    a.read_exact(&mut response).unwrap();
    assert_eq!(&response, b"ok");
}

#[test]
fn blocking_io_obeys_timeouts() {
    let (mut a, _b) = MemoryStream::pair().unwrap();
    a.set_read_timeout(Some(Duration::from_millis(15))).unwrap();
    assert_eq!(
        a.read(&mut [0]).unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
    a.write_all(&vec![1; CAPACITY]).unwrap();
    a.set_write_timeout(Some(Duration::from_millis(15)))
        .unwrap();
    assert_eq!(a.write(&[2]).unwrap_err().kind(), io::ErrorKind::TimedOut);
    assert_eq!(
        a.set_read_timeout(Some(Duration::ZERO)).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
}
