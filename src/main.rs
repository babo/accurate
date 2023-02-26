use crossterm::event::KeyCode;
use crossterm::terminal;
use sntpc::{Error, NtpContext, NtpTimestampGenerator, NtpUdpSocket, Result};

use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

use chrono::Utc;
use crossterm::{
    event::{
        poll, read, DisableMouseCapture, EnableMouseCapture, Event, MouseButton, MouseEventKind,
    },
    execute,
};

#[derive(Copy, Clone, Default)]
struct StdTimestampGen {
    duration: Duration,
}

impl NtpTimestampGenerator for StdTimestampGen {
    fn init(&mut self) {
        self.duration = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap();
    }

    fn timestamp_sec(&self) -> u64 {
        self.duration.as_secs()
    }

    fn timestamp_subsec_micros(&self) -> u32 {
        self.duration.subsec_micros()
    }
}

#[derive(Debug)]
struct UdpSocketWrapper(UdpSocket);

impl NtpUdpSocket for UdpSocketWrapper {
    fn send_to<T: ToSocketAddrs>(&self, buf: &[u8], addr: T) -> Result<usize> {
        match self.0.send_to(buf, addr) {
            Ok(usize) => Ok(usize),
            Err(_) => Err(Error::Network),
        }
    }

    fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr)> {
        match self.0.recv_from(buf) {
            Ok((size, addr)) => Ok((size, addr)),
            Err(_) => Err(Error::Network),
        }
    }
}

fn wait_for_click() -> crossterm::Result<bool> {
    let start = Instant::now();

    loop {
        // `poll()` waits for an `Event` for a given time period
        if poll(Duration::from_millis(500))? {
            // It's guaranteed that the `read()` won't block when the `poll()`
            // function returns `true`
            match read()? {
                Event::Key(event) => {
                    if match event.code {
                        KeyCode::Esc => true,
                        KeyCode::Enter => true,
                        KeyCode::Char(' ') => true,
                        _ => false,
                    } {
                        return Ok(false);
                    }
                }
                Event::Mouse(event) => {
                    if event.kind == MouseEventKind::Down(MouseButton::Left) {
                        return Ok(true);
                    }
                }
                _ => (),
            }
        }
        if start.elapsed().as_secs() > 70 {
            println!("Still there?");
            return Ok(false);
        }
    }
}

async fn w() -> crossterm::Result<()> {
    let socket = UdpSocket::bind("0.0.0.0:0").expect("Unable to crate UDP socket");
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("Unable to set UDP socket read timeout");
    let sock_wrapper = UdpSocketWrapper(socket);
    let ntp_context = NtpContext::new(StdTimestampGen::default());
    let result = sntpc::get_time("time.google.com:123", sock_wrapper, ntp_context);
    match result {
        Ok(time) => {
            println!("Press the mouse when the seconds reach 12'clock position.");
            terminal::enable_raw_mode()?;
            let mut stdout = std::io::stdout();
            execute!(stdout, EnableMouseCapture)?;
            let start = Instant::now();

            let capture = match wait_for_click() {
                Ok(true) => Some(start.elapsed()),
                _ => None,
            };

            execute!(stdout, DisableMouseCapture)?;
            terminal::disable_raw_mode()?;

            match capture {
                Some(duration) => {
                    let u = Utc::now();
                    println!(
                        "Got time: {}.{} vs {}",
                        time.sec(),
                        time.sec_fraction(),
                        u.timestamp()
                    );
                    println!(
                        "Pressed after {}s {}ms",
                        duration.as_secs(),
                        duration.as_millis()
                    );
                }
                _ => println!("Next time!"),
            };
        }
        Err(err) => println!("Err: {:?}", err),
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    let _a = w().await;
}
