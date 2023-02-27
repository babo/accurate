use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

use chrono::prelude::*;

use clap::Parser;
use crossterm::{
    event::{
        poll, read, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseButton,
        MouseEventKind,
    },
    execute, terminal,
};
use rusqlite::{Connection, Result as SQLResult};
use sntpc::{Error, NtpContext, NtpTimestampGenerator, NtpUdpSocket, Result as SNTPResult};

const DEFAULT_NAME: &str = "main";
const DEFAULT_DATABASE: &str = "watch.sqlite";

/// Measure your watch accuracy on the long run
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Synchronize your watch
    #[arg(short, long, default_value_t = false)]
    sync: bool,

    /// Name of the watch to measure
    #[arg(short, long, default_value_t = DEFAULT_NAME.to_string())]
    name: String,

    /// Database file
    #[arg(short, long, default_value_t = DEFAULT_DATABASE.to_string())]
    data: String,

    /// Number of times to greet
    #[arg(short, long, default_value_t = 1)]
    count: u8,
}

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
    fn send_to<T: ToSocketAddrs>(&self, buf: &[u8], addr: T) -> SNTPResult<usize> {
        match self.0.send_to(buf, addr) {
            Ok(usize) => Ok(usize),
            Err(_) => Err(Error::Network),
        }
    }

    fn recv_from(&self, buf: &mut [u8]) -> SNTPResult<(usize, SocketAddr)> {
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

fn save_to(
    dbname: &str,
    sec: u32,
    ms: u32,
    duration: u128,
    name: &str,
    sync: bool,
) -> SQLResult<()> {
    let conn = Connection::open(dbname)?;
    conn.path().map(|path| {
        println!("Path: {:?}", path.as_os_str());
    });
    conn.execute(
        "CREATE TABLE IF NOT EXISTS measurements (
            ts   INTEGER PRIMARY KEY,
            diff INTEGER NOT NULL,
            sync BOOLEAN,
            name TEXT NOT NULL
        )",
        (), // empty list of parameters.
    )?;

    let sec: i64 = (duration / 1000) as i64 + sec as i64;
    let ms: u32 = ms + (duration % 1000) as u32;
    let dt = Utc.timestamp_opt(sec, ms * 1_000_000u32);

    dt.single().map(|w| {
        let m = w.minute();
        let s = w.second() as i32;
        let d = if s < 30 { -s } else { 60 - s };
        println!("Single: {m} {s} {d}");
        match conn.execute(
            "INSERT INTO measurements(ts, diff, sync, name) VALUES(?1,?2,?3,?4)",
            (sec, d, sync, &name.to_string()),
        ) {
            Ok(up) => println!("Updated: {up}"),
            Err(e) => println!("Error: {e}"),
        }
    });

    Ok(())
}

async fn worker(args: &Args) -> crossterm::Result<()> {
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
                    let ms = (time.sec_fraction() as u64) * 1000 / u32::MAX as u64;
                    let res = save_to(
                        args.data.as_str(),
                        time.sec(),
                        ms as u32,
                        duration.as_millis(),
                        args.name.as_str(),
                        args.sync,
                    );
                    let _ = res.map_err(|err| {
                        println!("An error occured: {}", err.to_string());
                    });
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
    let args = Args::parse();
    let _a = worker(&args).await;
}
