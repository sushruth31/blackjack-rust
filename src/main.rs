//! TCP front end for the blackjack table.
//!
//! One task owns the [`Table`] and is the only thing that mutates it, so the
//! rules need no locks. Every connection is two tasks — a reader that turns
//! lines into commands and a writer that drains a bounded mailbox — which
//! avoids `select!` over [`AsyncBufReadExt::lines`] and the partial-line
//! problems that come with cancelling a read.

use blackjack_rust::config::Config;
use blackjack_rust::protocol::{self, GREETING, HELP};
use blackjack_rust::rules::Rules;
use blackjack_rust::table::{Command, Event, SeatId, Table, TableError};
use rand::rngs::StdRng;
use rand::SeedableRng;
use std::collections::HashMap;
use std::error::Error;
use std::io;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{self, Receiver, Sender};

/// Bounded so that one wedged client cannot make the table allocate forever.
const MAILBOX_DEPTH: usize = 64;
const MAX_NAME_LEN: usize = 16;

type Clients = HashMap<SeatId, Sender<String>>;

enum Input {
    Join { name: String, outbox: Sender<String> },
    Play(Command),
    Leave,
}

struct Request {
    seat: SeatId,
    input: Input,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("blackjack: {}", error);
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn Error>> {
    let config = Config::from_env()?;
    let listener = TcpListener::bind(&config.bind_addr).await?;
    println!("blackjack table open on {}", config.bind_addr);
    let (requests, inbox) = mpsc::channel(MAILBOX_DEPTH);
    tokio::spawn(run_table(config.rules, inbox));
    loop {
        let (stream, peer) = listener.accept().await?;
        tokio::spawn(serve(stream, peer.to_string(), requests.clone()));
    }
}

/// The single owner of the table. Commands arrive in order, so the rules run
/// on one thread with no shared mutable state at all.
async fn run_table(rules: Rules, mut inbox: Receiver<Request>) {
    let mut table = Table::new(rules, StdRng::from_entropy());
    let mut clients = Clients::new();
    while let Some(Request { seat, input }) = inbox.recv().await {
        let result = dispatch(&mut table, &mut clients, &seat, input);
        publish(&clients, &seat, result);
    }
}

fn dispatch(
    table: &mut Table<StdRng>,
    clients: &mut Clients,
    seat: &str,
    input: Input,
) -> Result<Vec<Event>, TableError> {
    match input {
        Input::Join { name, outbox } => {
            clients.insert(seat.to_string(), outbox);
            table.join(seat, &name)
        }
        Input::Play(command) => table.apply(seat, command),
        Input::Leave => {
            clients.remove(seat);
            Ok(table.leave(seat))
        }
    }
}

fn publish(clients: &Clients, origin: &str, result: Result<Vec<Event>, TableError>) {
    let events = match result {
        Ok(events) => events,
        Err(error) => return whisper(clients, origin, &format!("{}\n", error)),
    };
    events.iter().for_each(|event| announce(clients, &protocol::render(event)));
}

/// Never awaits: a client that has stopped reading loses lines rather than
/// stalling the round for everyone else.
fn announce(clients: &Clients, line: &str) {
    clients.values().for_each(|outbox| {
        let _ = outbox.try_send(line.to_string());
    });
}

fn whisper(clients: &Clients, seat: &str, line: &str) {
    if let Some(outbox) = clients.get(seat) {
        let _ = outbox.try_send(line.to_string());
    }
}

async fn serve(stream: TcpStream, seat: SeatId, requests: Sender<Request>) {
    let (reader, writer) = stream.into_split();
    let (outbox, mailbox) = mpsc::channel(MAILBOX_DEPTH);
    tokio::spawn(drain(mailbox, writer));
    let _ = outbox.try_send(GREETING.to_string());
    if let Err(error) = read_commands(reader, &seat, &requests, outbox).await {
        eprintln!("connection {} closed: {}", seat, error);
    }
    submit(&requests, &seat, Input::Leave).await;
}

async fn drain(mut mailbox: Receiver<String>, mut writer: OwnedWriteHalf) {
    while let Some(line) = mailbox.recv().await {
        if writer.write_all(line.as_bytes()).await.is_err() {
            return;
        }
    }
}

async fn read_commands(
    reader: OwnedReadHalf,
    seat: &str,
    requests: &Sender<Request>,
    outbox: Sender<String>,
) -> io::Result<()> {
    let mut lines = BufReader::new(reader).lines();
    let Some(greeting) = lines.next_line().await? else {
        return Ok(());
    };
    let name = seat_name(&greeting, seat);
    let _ = outbox.try_send(HELP.to_string());
    submit(requests, seat, Input::Join { name, outbox: outbox.clone() }).await;
    while let Some(line) = lines.next_line().await? {
        forward(&line, seat, requests, &outbox).await;
    }
    Ok(())
}

async fn forward(line: &str, seat: &str, requests: &Sender<Request>, outbox: &Sender<String>) {
    let Some(command) = protocol::parse_command(line) else {
        let _ = outbox.try_send(HELP.to_string());
        return;
    };
    submit(requests, seat, Input::Play(command)).await;
}

/// Awaits on purpose: inbound backpressure should slow a chatty client down.
async fn submit(requests: &Sender<Request>, seat: &str, input: Input) {
    let _ = requests.send(Request { seat: seat.to_string(), input }).await;
}

fn seat_name(raw: &str, seat: &str) -> String {
    let name: String = raw.trim().chars().filter(|c| !c.is_control()).take(MAX_NAME_LEN).collect();
    if name.is_empty() {
        return format!("guest-{}", seat);
    }
    name
}
