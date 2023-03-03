use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::Duration;

use chrono::prelude::*;

use clap::Parser;
use rusqlite::{Connection, Result as SQLResult};
use sntpc::{Error, NtpContext, NtpResult, NtpTimestampGenerator, NtpUdpSocket};

use cursive::views::{Dialog, DummyView, LinearLayout, RadioGroup};

const DEFAULT_NAME: &str = "main";
const DEFAULT_DATABASE: &str = "watch.sqlite";
const DEFAULT_COMMENT: &str = "";

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

    /// Comment of the measurement if any
    #[arg(short, long, default_value_t = DEFAULT_COMMENT.to_string())]
    comment: String,
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
    fn send_to<T: ToSocketAddrs>(&self, buf: &[u8], addr: T) -> sntpc::Result<usize> {
        match self.0.send_to(buf, addr) {
            Ok(usize) => Ok(usize),
            Err(_) => Err(Error::Network),
        }
    }

    fn recv_from(&self, buf: &mut [u8]) -> sntpc::Result<(usize, SocketAddr)> {
        match self.0.recv_from(buf) {
            Ok((size, addr)) => Ok((size, addr)),
            Err(_) => Err(Error::Network),
        }
    }
}

fn save_to(
    dbname: &str,
    ts: u32,
    delta: i32,
    name: &String,
    comment: &String,
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
            name TEXT NOT NULL,
            comment TEXT NULL
        );",
        (), // empty list of parameters.
    )?;

    let mut stmt = conn.prepare("select count(*) from measurements;")?;
    let mut rows = stmt.query(())?;
    let first = rows.next()?.unwrap();
    let n: usize = first.get(0)?;
    let sync = sync || n == 0;

    match conn.execute(
        "INSERT INTO measurements(ts, diff, sync, name, comment) VALUES(?1,?2,?3,?4,?5)",
        (ts, delta, sync, name, comment),
    ) {
        Ok(up) => println!("Updated: {up}"),
        Err(e) => println!("Error: {e}"),
    }

    Ok(())
}

async fn gui(args: &Args) -> Result<(), Error> {
    let mut siv = cursive::default();

    siv.add_global_callback(cursive::event::Key::Esc, cursive::Cursive::quit);

    siv.add_layer(
        Dialog::text("Press the mouse when the seconds reach 12'clock position.")
            .button("12", |s| s.quit()),
    );

    let ref_time = get_ntp_time().await?;
    let start = chrono::Utc::now();
    siv.run();
    let click = chrono::Utc::now();
    let duration = click.signed_duration_since(start);

    let sec = ref_time.sec();
    let ms = (ref_time.sec_fraction() as u64) * 1000 / u32::MAX as u64;
    let click_dt = Utc
        .timestamp_opt(sec as i64, (ms * 1_000_000u64) as u32)
        .single()
        .expect("Unuable to convert timestamp")
        .checked_add_signed(duration)
        .expect("Failed to add duration");

    let mut minute_group: RadioGroup<i32> = RadioGroup::new();

    let tm1 = (click_dt.minute() + 59) % 60;
    let tp1 = (click_dt.minute() + 1) % 60;

    siv.pop_layer();
    siv.add_layer(
        Dialog::new()
            .title("Please select the minute")
            // We'll have two columns side-by-side
            .content(
                LinearLayout::vertical()
                    .child(
                        LinearLayout::vertical()
                            // The color group uses the label itself as stored value
                            // By default, the first item is selected.
                            .child(minute_group.button(-60, format!("{}", tm1)))
                            .child(minute_group.button(0, format!("{}", click_dt.minute())))
                            .child(minute_group.button(60, format!("{}", tp1))),
                    )
                    // A DummyView is used as a spacer
                    .child(DummyView),
            )
            .button("Ok", cursive::Cursive::quit),
    );
    siv.run();

    let delta = minute_group
        .selection()
        .checked_sub(click_dt.second() as i32);
    match delta {
        Some(delta) => {
            let res = save_to(
                args.data.as_str(),
                sec,
                delta,
                &args.name,
                &args.comment,
                args.sync,
            );
            let _ = res.map_err(|err| {
                println!("An error occured: {}", err.to_string());
            });
            siv.pop_layer();
            // And we simply print the result.
            let text = format!("Difference is {:?}s", delta);
            siv.add_layer(Dialog::text(text).button("Ok", |s| s.quit()));
            siv.run();
        }
        None => (),
    }

    Ok(())
}

async fn get_ntp_time() -> Result<NtpResult, Error> {
    let socket = UdpSocket::bind("0.0.0.0:0").expect("Unable to crate UDP socket");
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("Unable to set UDP socket read timeout");
    let sock_wrapper = UdpSocketWrapper(socket);
    let ntp_context = NtpContext::new(StdTimestampGen::default());
    sntpc::get_time("time.cloudflare.com:123", sock_wrapper, ntp_context)
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let _a = gui(&args).await;
}
