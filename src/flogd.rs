use clap::Parser;
use daemonize::Daemonize;
use flib::*;
use inotify::{EventMask, Inotify, WatchMask};
use log::{error, info, warn, LevelFilter};
use std::collections::HashMap;
use std::ffi::CString;
use std::io::Read;
use std::os::raw::{c_char, c_int};
use std::os::unix::net::UnixListener;
use std::path::Path;
use syslog::BasicLogger;
use syslog::{Facility, Formatter3164};

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
const SOCK_PATH: &str = "/var/run/flogd.socket";
fn main() {
    let args = Args::parse();

    try_init();

    let mut state = load_index();
    let mut inotify = Inotify::init().expect("Failed to intialize inotify object");
    let mut wdm = HashMap::new();
    for (k, _) in state.files.clone() {
        let wd = inotify
            .add_watch(&k, WatchMask::CLOSE_WRITE)
            .unwrap_or_else(|_| panic!("Failed to add watch for {}", k));

        wdm.insert(wd, k);
    }

    unsafe {
        let sock_path_c = CString::new(SOCK_PATH).unwrap();
        unlink(sock_path_c.as_ptr());
    }
    let listener = UnixListener::bind(SOCK_PATH).unwrap();
    let daemonize = Daemonize::new()
        .pid_file(args.pid_file)
        .chown_pid_file(true)
        .working_directory(args.working_directory)
        .user(args.user.as_str())
        .group(args.group.as_str())
        .umask(0o777);

    match daemonize.start() {
        Ok(_) => println!("Success, daemonized"),
        Err(e) => panic!("Error, {}", e),
    }

    let formatter = Formatter3164 {
        facility: Facility::LOG_DAEMON,
        hostname: None,
        process: "flog".into(),
        pid: 0,
    };

    let logger = syslog::unix(formatter).expect("Failed to open syslog");

    log::set_boxed_logger(Box::new(BasicLogger::new(logger)))
        .map(|()| log::set_max_level(LevelFilter::Info))
        .expect("Failed to setup logger");

    match listener.accept() {
        Ok((mut socket, addr)) => {
            info!("Got a client: {:?}", addr);
            let mut resp = String::new();
            socket.read_to_string(&mut resp).unwrap();
            info!("Got message {}", resp);
        }
        Err(e) => error!("accept function failed: {:?}", e),
    }

    let mut buffer = [0; 1024];
    loop {
        let events = inotify.read_events_blocking(&mut buffer).unwrap();
        for e in events {
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
