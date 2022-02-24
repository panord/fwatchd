use anyhow::{anyhow, Context, Result};
use clap::Parser;
use crypto::digest::Digest;
use crypto::sha2;
use daemonize::Daemonize;
use flib::*;
use inotify::{EventMask, Inotify, WatchMask};
use log::{debug, error, info, warn, LevelFilter};
use nix::poll::{ppoll, PollFd, PollFlags};
use nix::sys::signal::SigSet;
use std::collections::HashMap;
use std::ffi::CString;
use std::io::{Read, Write};
use std::os::raw::{c_char, c_int};
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use syslog::BasicLogger;
use syslog::{Facility, Formatter3164};

const INDEX: &str = "/var/run/flog/index";
const INDEXD: &str = "/var/run/flog/index.d";

pub fn load_index() -> State {
    State::load(INDEX).unwrap_or_else(|_| State::new())
}

#[derive(Parser, Debug)]
#[clap(about, version, author)]
struct Args {
    /// Persistently try to setup a new inotify watch when IN_IGNORE event is received
    #[clap(short, long)]
    persistent: bool,
    #[clap(short, long, default_value = "/var/run/flog.pid")]
    pid_file: String,
    #[clap(short, long, default_value = "flog")]
    user: String,
    #[clap(short, long, default_value = "flog")]
    group: String,
    #[clap(short, long, default_value = "/var/run/flog")]
    working_directory: String,
    #[clap(long)]
    foreground: bool,
}

pub fn try_init() {
    if !Path::new(INDEXD).is_dir() {
        std::fs::create_dir_all(INDEXD).unwrap();
        State::new().save(INDEX).unwrap();
    }
}

extern "C" {
    fn unlink(c_int: *const c_char) -> c_int;
}

pub fn do_append(state: &mut State, fname: &str) -> Result<()> {
    let mut hasher = sha2::Sha256::new();
    let mut contents = String::new();
    let fpath = std::path::Path::new(&fname);
    let mut file = std::fs::File::open(&fpath)?;

    file.read_to_string(&mut contents)?;
    hasher.input_str(&contents);
    let target = format!("{}/{}-{}", INDEXD, fpath.display(), hasher.result_str());
    println!("{}", &target);
    std::fs::create_dir_all(&std::path::Path::new(&target).parent().unwrap())?;
    std::fs::copy(&fpath, &target).expect("Failed to save file version");

    state
        .files
        .entry(fpath.display().to_string())
        .or_insert_with(HashMap::default)
        .insert(hasher.result_str(), target);

    state.save(INDEX)?;
    Ok(())
}

fn echoerr(pkt: &Packet) -> Result<Vec<u8>> {
    let msg = bincode::deserialize::<String>(&pkt.payload).context("Failed to deserialize")?;
    Err(anyhow!(msg))
}

fn echo(pkt: &Packet) -> Result<Vec<u8>> {
    let msg = bincode::deserialize::<String>(&pkt.payload).context("Failed to deserialize")?;
    Ok(bincode::serialize(&msg)?)
}

fn list(state: &State, pkt: &Packet) -> Result<Vec<u8>> {
    let fname = bincode::deserialize::<String>(&pkt.payload).context("Failed to deserialize")?;
    let resp = match fname.as_str() {
        "*" => format!("{:#?}", state.files).as_bytes().to_vec(),
        _ => {
            let h = state
                .files
                .get(&fname)
                .context("Found no such tracked file")?;
            format!("{:?}", h).as_bytes().to_vec()
        }
    };
    Ok(resp)
}

fn track(state: &mut State, pkt: &Packet) -> Result<Vec<u8>> {
    let fname = bincode::deserialize::<String>(&pkt.payload).context("Failed to deserialize")?;

    do_append(state, &fname)?;
    Ok(format!("Added {} to tracked files", fname)
        .as_bytes()
        .to_vec())
}

fn process(socket: &mut UnixStream, state: &mut State) -> bool {
    let mut buf: [u8; 1024] = [0; 1024];
    let mut reload = false;
    // This should read only as much data as we are
    // expecting, perhaps via serialize_size?
    if socket.read(&mut buf).is_ok() {
        let pkt = bincode::deserialize::<Packet>(&buf).unwrap();
        let res = match pkt.command {
            Command::ECHOERR => echoerr(&pkt),
            Command::ECHO => echo(&pkt),
            Command::LIST => list(state, &pkt),
            Command::TRACK => {
                reload = true;
                track(state, &pkt)
            }
        };

        match res {
            Ok(resp) => {
                debug!("Responding to request {:#?}", pkt.command);
                socket
                    .write_all(&resp)
                    .context("Failed to write to socket")
                    .unwrap()
            }
            Err(msg) => {
                let msg = format!("Failed to process request {:#?}, {:?}", pkt.command, msg);
                error!("{}", msg);
                socket
                    .write_all(msg.as_bytes())
                    .context("Failed to write to socket")
                    .unwrap();
            }
        };
    }
    reload
}

fn listen(listener: &UnixListener, state: &mut State) -> bool {
    match listener.accept() {
        Ok((mut s, _)) => {
            info!(
                "Processing request on {:#?} from {:#?}",
                s.local_addr(),
                s.peer_addr()
            );
            process(&mut s, state)
        }
        Err(msg) => {
            error!("{}", msg);
            false
        }
    }
}

fn main() {
    let args = Args::parse();

    try_init();

    let mut state = load_index();
    let daemonize = Daemonize::new()
        .pid_file(args.pid_file)
        .chown_pid_file(true)
        .working_directory(args.working_directory)
        .user(args.user.as_str())
        .group(args.group.as_str())
        .umask(0o777);

    if !args.foreground {
        match daemonize.start() {
            Ok(_) => println!("Success, daemonized"),
            Err(e) => error!("Failed to daemonize: {}", e),
        }
    }

    let formatter = Formatter3164 {
        facility: Facility::LOG_DAEMON,
        hostname: None,
        process: "flog".into(),
        pid: 0,
    };
    let mut wdm = HashMap::new();
    let logger = syslog::unix(formatter).expect("Failed to open syslog");

    log::set_boxed_logger(Box::new(BasicLogger::new(logger)))
        .map(|()| log::set_max_level(LevelFilter::Debug))
        .expect("Failed to setup logger");

    let mut buffer = [0; 1024];
    unsafe {
        let sock_path_c = CString::new(SOCK_PATH).unwrap();
        unlink(sock_path_c.as_ptr());
    }
    let listener = UnixListener::bind(SOCK_PATH).unwrap();
    let mut inotify = Inotify::init().expect("Failed to intialize inotify object");
    for (k, _) in state.files.clone() {
        let wd = inotify
            .add_watch(&k, WatchMask::CLOSE_WRITE)
            .unwrap_or_else(|_| panic!("Failed to add watch for {}", k));

        wdm.insert(wd, k);
    }
    let mut rfd: Vec<PollFd> = vec![listener.as_raw_fd(), inotify.as_raw_fd()]
        .iter()
        .map(|x| PollFd::new(*x, PollFlags::all()))
        .collect();

    loop {
        println!(
            "Events on {} fds",
            ppoll(rfd.as_mut_slice(), None, SigSet::all()).unwrap()
        );
        let reload = match rfd[0].revents() {
            Some(ev) => {
                if !ev.is_empty() {
                    println!("Event {:?} on UDS", ev);
                    // Should be no event here
                    listen(&listener, &mut state)
                } else {
                    false
                }
            }
            None => false,
        };

        if reload {
            println!("Reloading inotify watches!!!");
            for (k, _) in state.files.clone() {
                let wd = inotify
                    .add_watch(&k, WatchMask::CLOSE_WRITE)
                    .unwrap_or_else(|_| panic!("Failed to add watch for {}", k));

                wdm.insert(wd, k);
            }
            rfd = vec![listener.as_raw_fd(), inotify.as_raw_fd()]
                .iter()
                .map(|x| PollFd::new(*x, PollFlags::POLLIN))
                .collect();
        }

        let events = inotify.read_events(&mut buffer);
        if events.is_err() {
            continue;
        }

        for e in events.unwrap() {
            println!("Processing inotify event");
            let name = wdm[&e.wd].clone();
            if args.persistent && e.mask == EventMask::IGNORED {
                if let Ok(wd) = inotify.add_watch(&name, WatchMask::CLOSE_WRITE) {
                    wdm.remove(&e.wd);
                    wdm.insert(wd, name.clone());
                }
                warn!("Failed to add watch for {}", name);
            }

            do_append(&mut state, &name).unwrap();
            state.save(INDEX).unwrap();
        }
    }
}
