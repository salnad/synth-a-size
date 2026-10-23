//! Stoppable reads over real local sockets: a server that never answers,
//! one that answers late, and one that goes quiet.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use sas_llm_agent::stop::{StopCheck, StoppableStream, POLL, STOPPED};

/// A connected client, and the server's end, which `serve` runs with.
fn connect(serve: impl FnOnce(TcpStream) + Send + 'static) -> TcpStream {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    thread::spawn(move || serve(listener.accept().unwrap().0));
    TcpStream::connect(address).unwrap()
}

fn flag() -> (Arc<AtomicBool>, StopCheck) {
    let flag = Arc::new(AtomicBool::new(false));
    let check = flag.clone();
    (flag, Arc::new(move || check.load(Ordering::Relaxed)))
}

#[test]
fn a_stop_ends_a_read_the_server_never_answers() {
    let client = connect(|server| {
        thread::sleep(Duration::from_secs(5));
        drop(server);
    });
    let (stop, check) = flag();
    let mut stream = StoppableStream::new(client.try_clone().unwrap(), Some(&client), check);
    let stopper = stop.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(100));
        stopper.store(true, Ordering::Relaxed);
    });
    let started = Instant::now();
    let error = stream.read(&mut [0u8; 16]).unwrap_err();
    assert_eq!(error.to_string(), STOPPED);
    assert!(
        started.elapsed() < Duration::from_millis(100) + 2 * POLL,
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn an_answer_slower_than_the_poll_still_arrives() {
    let client = connect(|mut server| {
        thread::sleep(3 * POLL);
        server.write_all(b"answer").unwrap();
    });
    let (_stop, check) = flag();
    let mut stream = StoppableStream::new(client.try_clone().unwrap(), Some(&client), check);
    let mut buf = [0u8; 6];
    stream.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"answer");
}

#[test]
fn a_quiet_server_hits_the_idle_limit() {
    let client = connect(|server| {
        thread::sleep(Duration::from_secs(5));
        drop(server);
    });
    let (_stop, check) = flag();
    let mut stream = StoppableStream::new(client.try_clone().unwrap(), Some(&client), check)
        .with_idle(Duration::from_millis(600));
    let started = Instant::now();
    let error = stream.read(&mut [0u8; 16]).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert!(started.elapsed() >= Duration::from_millis(600));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn a_stop_before_reading_returns_at_once() {
    let client = connect(|server| {
        thread::sleep(Duration::from_secs(5));
        drop(server);
    });
    let (stop, check) = flag();
    stop.store(true, Ordering::Relaxed);
    let mut stream = StoppableStream::new(client.try_clone().unwrap(), Some(&client), check);
    let started = Instant::now();
    assert!(stream.read(&mut [0u8; 16]).is_err());
    assert!(started.elapsed() < POLL);
}
