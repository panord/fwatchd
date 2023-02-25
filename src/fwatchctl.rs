mod socket;
use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use socket::*;
use std::io::prelude::*;
use std::os::unix::net::UnixStream;

fn track(args: &TrackArgs) -> Result<()> {
    let track = Track {
        fpath: args.file.clone(),
        alias: Alias::Basename,
        action: Action::Save,
    };
    let payload = bincode::serialize(&track).context("Failed to serialize payload")?;
    let mut stream = UnixStream::connect(SOCK_PATH)?;
    let mut response = String::new();

    let pkt = Packet {
        command: socket::Command::Track,
        payload,
    };
    stream.write_all(&bincode::serialize(&pkt)?)?;
    stream.read_to_string(&mut response)?;
    println!("{}", response);
    Ok(())
}

fn list(args: &ListArgs) -> Result<()> {
    let payload: String = args.file.clone();
    let mut stream = UnixStream::connect(SOCK_PATH)?;
    let mut response = String::new();
    let payload = bincode::serialize(&payload).context("Failed to serialize payload")?;
    let pkt = Packet {
        command: socket::Command::List,
        payload,
    };

    stream.write_all(&bincode::serialize(&pkt)?)?;
    stream.read_to_string(&mut response)?;
    println!("{}", response);
    Ok(())
}

fn select(args: &SelectArgs) -> Result<()> {
    let sel: (String, String) = (args.file.clone(), args.hash.clone());
    let mut stream = UnixStream::connect(SOCK_PATH).context("Failed to open socket")?;
    let mut response = String::new();
    let payload = bincode::serialize(&sel).context("Failed to serialize payload")?;
    let pkt = Packet {
        command: socket::Command::Select,
        payload,
    };

    stream
        .write_all(&bincode::serialize(&pkt)?)
        .context("Failed to write to socket")?;
    stream
        .read_to_string(&mut response)
        .context("Failed to read from socket")?;

    println!("{}", response);
    Ok(())
}

fn echo(args: &EchoArgs, is_err: bool) -> Result<()> {
    let msg: String = args.message.clone();
    let mut stream = UnixStream::connect(SOCK_PATH)?;
    let mut response = String::new();
    let payload = bincode::serialize(&msg).context("Failed to serialize payload")?;
    let pkt = Packet {
        command: if is_err {
            socket::Command::Echoerr
        } else {
            socket::Command::Echo
        },
        payload,
    };

    stream.write_all(&bincode::serialize(&pkt)?)?;
    stream.read_to_string(&mut response)?;
    println!("{}", response);
    Ok(())
}

#[derive(Parser, Debug, Clone)]
struct SelectArgs {
    #[arg(short, long)]
    file: String,
    #[arg(short, long)]
    hash: String,
}

#[derive(Parser, Debug, Clone)]
struct ListArgs {
    #[arg(short, long)]
    file: String,
}

#[derive(Parser, Debug, Clone)]
struct TrackArgs {
    #[arg(short, long)]
    file: String,
}

#[derive(Parser, Debug, Clone)]
struct EchoArgs {
    #[arg(short, long)]
    message: String,
}

#[derive(Subcommand, Debug, Clone)]
enum CtlCommand {
    Track(TrackArgs),
    List(ListArgs),
    Select(SelectArgs),
    Echo(EchoArgs),
    EchoErr(EchoArgs),
}

#[derive(Parser, Debug)]
#[clap(about, version, author)]
struct Args {
    #[command(subcommand)]
    command: CtlCommand,
}

fn main() {
    let app = Args::parse();
    match app.command {
        CtlCommand::Track(args) => track(&args),
        CtlCommand::Select(args) => select(&args),
        CtlCommand::List(args) => list(&args),
        CtlCommand::Echo(args) => echo(&args, false),
        CtlCommand::EchoErr(args) => echo(&args, true),
        #[allow(unreachable_patterns)]
        _ => Err(anyhow!("Unrecognized command")),
    }
    .unwrap();
}
