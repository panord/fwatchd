use anyhow::{anyhow, Context, Result};
use clap::Parser;
use crypto::digest::Digest;
use crypto::sha2;
use daemonize::Daemonize;
use flib::*;
use inotify::{EventMask, Inotify, WatchMask};
use log::{debug, error, info, warn, Level, LevelFilter};
#[cfg(target_os = "macos")]
use nix::poll::poll;
#[cfg(target_os = "linux")]
use nix::poll::ppoll;
use nix::poll::{PollFd, PollFlags};
#[cfg(target_os = "linux")]
use nix::sys::signal::SigSet;
use nix::unistd::{chown, unlink, Gid, Uid};
use signal_hook::flag;
use std::collections::HashMap;
use std::ffi::CString;
use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{atomic::AtomicBool, atomic::Ordering, Arc};
use syslog::{BasicLogger, Facility, Formatter3164};

const INDEX: &str = "/var/run/fwatch/index";
const INDEXD: &str = "/var/run/fwatch/index.d";

pub fn load_index(workdir: &str) -> State {
    State::load(&format!("{workdir}/index")).unwrap_or_else(|_| State::new())
}

#[derive(Parser, Debug)]
#[clap(about, version, author)]
struct Args {
    /// Persistently try to setup a new inotify watch when IN_IGNORE event is received
    #[clap(short, long)]
    persistent: bool,
    #[clap(long, default_value = "/var/run/fwatch.pid")]
    pid_file: String,
    #[clap(short, long, default_value = "fwatch")]
    user: String,
    #[clap(short, long, default_value = "fwatch")]
    group: String,
    #[clap(short, long, default_value = "/var/run/fwatch")]
    working_directory: String,
    #[clap(long)]
    foreground: bool,
}

fn script(fpath: &str, spath: &str) -> Result<()> {
    std::process::Command::new(spath)
        .arg(fpath)
        .spawn()
        .context(format!("Failed to execute {}", spath))?;

    Ok(())
}

fn sha256sum(fpath: &Path) -> Result<String> {
    let mut contents = String::new();
    let mut hasher = sha2::Sha256::new();
    let mut file = std::fs::File::open(&fpath)?;

    file.read_to_string(&mut contents)?;
    hasher.input_str(&contents);
    Ok(hasher.result_str())
}

fn save(state: &mut State, fname: &str, alias: &Alias) -> Result<()> {
    let fpath = std::path::Path::new(&fname);

    let astr = match alias.clone() {
        Alias::BASENAME => Path::new(&fname)
            .file_name()
            .ok_or_else(|| anyhow!("Could not determine basename"))?
            .to_str()
            .ok_or_else(|| anyhow!("Could not convert to str"))?
            .to_string(),
        Alias::NAME(name) => name,
        Alias::SCRIPT(spath) => String::from_utf8(
            std::process::Command::new(spath)
                .arg(&fname)
                .output()?
                .stdout,
        )?
        .trim()
        .to_string(),
    };
    let hash = sha256sum(fpath)?;
    let target = format!("{}/{}-{}", INDEXD, fpath.display(), &hash);
    std::fs::create_dir_all(
        &std::path::Path::new(&target)
            .parent()
            .context("Failed to create directory for target")
            .unwrap(),
    )?;
    std::fs::copy(&fpath, &target).context("Failed to save file version")?;
    state
        .files
        .entry(fpath.display().to_string())
        .or_insert(Entry {
            action: Action::SAVE,
            alias: alias.clone(),
            snapshots: HashMap::default(),
        })
        .snapshots
        .insert(hash, (astr, target));

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
        "*" => {
            let mut tmp = vec![];
            for (k, v) in &state.files {
                for (ks, vs) in &v.snapshots {
                    let selected = if let Ok(hash) = sha256sum(Path::new(&k)) {
                        hash == *ks
                    } else {
                        false
                    };
                    if selected {
                        tmp.append(&mut format!("{k} {ks:32} ({}) *\n", vs.0).as_bytes().to_vec());
                    } else {
                        tmp.append(&mut format!("{k} {ks:32} ({})\n", vs.0).as_bytes().to_vec());
                    }
                }
            }
            tmp
        }
        _ => {
            let h = state
                .files
                .get(&fname)
                .context("Found no such tracked file")?;

            let mut tmp = vec![];
            for (ks, vs) in &h.snapshots {
                let selected = if let Ok(hash) = sha256sum(Path::new(&fname)) {
                    hash == *ks
                } else {
                    false
                };
                if selected {
                    tmp.append(
                        &mut format!("{fname} {ks:32} ({}) *\n", vs.0)
                            .as_bytes()
                            .to_vec(),
                    );
                } else {
                    tmp.append(&mut format!("{fname} {ks:32} ({})\n", vs.0).as_bytes().to_vec());
                }
            }
            tmp
        }
    };
    Ok(resp)
}

fn select(state: &State, pkt: &Packet) -> Result<Vec<u8>> {
    let (fpath, hash) =
        bincode::deserialize::<(String, String)>(&pkt.payload).context("Failed to deserialize")?;

    let nfpath = state
        .files
        .get(&fpath)
        .context("Found no such tracked file")?
        .snapshots
        .get(&hash)
        .context("Found no such file version")?
        .clone()
        .1;

    std::fs::copy(&nfpath, &fpath).context("Failed to copy file {nfpath} to {fpath}")?;
    Ok(format!("Selected {nfpath} ==> {fpath}").as_bytes().to_vec())
}

fn track(state: &mut State, pkt: &Packet) -> Result<Vec<u8>> {
    let track = bincode::deserialize::<Track>(&pkt.payload).context("Failed to deserialize")?;

    save(state, &track.fpath, &track.alias)?;
    state
        .files
        .entry(track.fpath.clone())
        .and_modify(|x| x.action = track.action.clone());
    Ok(format!(
        "Added {} with action {:?} and alias method {:?} to tracked files",
        &track.fpath, &track.action, &track.alias,
    )
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
            Command::SELECT => {
                reload = true;
                select(state, &pkt)
            }
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
                s.local_addr().unwrap(),
                s.peer_addr().unwrap()
            );
            process(&mut s, state)
        }
        Err(msg) => {
            error!("{}", msg);
            false
        }
    }
}

fn action(state: &mut State, fname: &str) -> Result<()> {
    let entry = &state.files[fname].clone();

    info!("Action {:?} on {:?}", &entry.action, &fname);
    match &entry.action {
        Action::SAVE => save(state, fname, &entry.alias),
        Action::SCRIPT(spath) => script(fname, spath),
    }
}

fn main() {
    let args = Args::parse();
    let _ = unlink(SOCK_PATH);
    let listener = UnixListener::bind(SOCK_PATH).unwrap();

    let wdir = args.working_directory.clone();
    let ddir = PathBuf::from(&wdir);
    unsafe {
        let uname = CString::new(args.user.clone())
            .context("Found no such user")
            .unwrap();

        let pw = libc::getpwnam(uname.as_ptr());
        if pw.is_null() {
            println!("No such user {}", args.user);
            return;
        }
        let uid: u32 = (*pw).pw_uid;
        let gid: u32 = (*pw).pw_gid;

        std::fs::create_dir_all(args.working_directory.clone())
            .context("Failed to create runtime directory")
            .unwrap();

        let daemonize = Daemonize::new()
            .chown_pid_file(true)
            .working_directory(&ddir)
            .user(uid)
            .group(gid)
            .privileged_action(move || {
                chown(&ddir, Some(Uid::from_raw(uid)), Some(Gid::from_raw(gid)))
                    .context("Failed to change owner/group of working directory")
                    .unwrap();
            });

        if !args.foreground {
            match daemonize.start() {
                Ok(_) => {}
                Err(e) => println!("Failed to daemonize: {}", e),
            }
        }
    };

    std::fs::create_dir_all(Path::new(&args.working_directory).join("index.d"))
        .context("Failed to create runtime directory")
        .unwrap();

    if args.foreground {
        log::set_logger(&StdoutLog {
            level: Level::Debug,
        })
        .expect("Failed to setup logger");
        log::set_max_level(LevelFilter::Debug);
    } else {
        let formatter = Formatter3164 {
            facility: Facility::LOG_DAEMON,
            hostname: None,
            process: "fwatch".into(),
            pid: 0,
        };
        let logger = syslog::unix(formatter).expect("Failed to open syslog");

        log::set_boxed_logger(Box::new(BasicLogger::new(logger)))
            .map(|()| log::set_max_level(LevelFilter::Debug))
            .expect("Failed to setup logger");
    };

    let mut state = load_index(&args.working_directory);
    let mut wdm = HashMap::new();
    let mut buffer = [0; 1024];
    let mut inotify = Inotify::init().expect("Failed to intialize inotify object");
    for (k, _) in state.files.clone() {
        if let Ok(wd) = inotify.add_watch(&k, WatchMask::CLOSE_WRITE) {
            wdm.insert(wd, k);
        } else {
            error!("Failed to add watch for {k}");
        }
    }

    let mut rfd: Vec<PollFd> = vec![listener.as_raw_fd(), inotify.as_raw_fd()]
        .iter()
        .map(|x| PollFd::new(*x, PollFlags::all()))
        .collect();

    let term = Arc::new(AtomicBool::new(false));
    let hup = Arc::new(AtomicBool::new(false));

    flag::register(signal_hook::consts::SIGTERM, Arc::clone(&term)).unwrap();
    flag::register(signal_hook::consts::SIGINT, Arc::clone(&term)).unwrap();
    flag::register(signal_hook::consts::SIGHUP, Arc::clone(&hup)).unwrap();
    loop {
        #[cfg(target_os = "linux")]
        let _ = ppoll(rfd.as_mut_slice(), None, SigSet::empty());

        #[cfg(target_os = "macos")]
        let _ = poll(rfd.as_mut_slice(), 0).unwrap();

        let mut reload = match rfd[0].revents() {
            Some(ev) => {
                if !ev.is_empty() {
                    listen(&listener, &mut state)
                } else {
                    false
                }
            }
            None => false,
        };

        if term.load(Ordering::Relaxed) {
            info!("Received SIGTERM | SIGINT, exiting");
            break;
        }

        if hup.load(Ordering::Relaxed) {
            info!("Received SIGHUP, reloading index");
            // XXX: Please observe that this discards the current
            // state. This is likely not desired during normal execution
            state = load_index(&args.working_directory);
            reload = true;
        }

        if reload {
            info!("Reloading inotify watches");
            for (k, _) in state.files.clone() {
                if let Ok(wd) = inotify.add_watch(&k, WatchMask::CLOSE_WRITE) {
                    wdm.insert(wd, k);
                } else {
                    error!("Failed to add watch for {k}");
                }
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

        for e in events.context("Events error").unwrap() {
            debug!("Processing inotify event {:?}", e);
            let name = wdm[&e.wd].clone();
            if args.persistent && e.mask == EventMask::IGNORED {
                if let Ok(wd) = inotify.add_watch(&name, WatchMask::CLOSE_WRITE) {
                    wdm.remove(&e.wd);
                    wdm.insert(wd, name.clone());
                } else {
                    warn!("Failed to add watch for {}", name);
                }
            }

            match action(&mut state, &name) {
                Ok(_) => {}
                Err(msg) => error!("{}", msg),
            };
        }
    }
    state.save(INDEX).unwrap();
}
