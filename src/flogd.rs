use clap::Parser;

use daemonize::Daemonize;
use flib::*;
use inotify::{EventMask, Inotify, WatchMask};
use std::collections::HashMap;
use std::path::Path;

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

    let daemonize = Daemonize::new()
        .pid_file(args.pid_file)
        .chown_pid_file(true)
        .working_directory(args.working_directory)
        .user(args.user.as_str())
        .group(args.group.as_str())
        .umask(0o777);

    match daemonize.start() {
        Ok(_) => println!("Success, daemonized"),
        Err(e) => eprintln!("Error, {}", e),
    }

    let mut buffer = [0; 1024];
    loop {
        let events = inotify.read_events_blocking(&mut buffer).unwrap();
        for e in events {
            let name = wdm[&e.wd].clone();

            if args.persistent && e.mask == EventMask::IGNORED {
                let wd = inotify
                    .add_watch(&name, WatchMask::CLOSE_WRITE)
                    .unwrap_or_else(|_| panic!("Failed to add watch for {}", name));

                wdm.remove(&e.wd);
                wdm.insert(wd, name.clone());
            }

            do_append(&mut state, &name).unwrap();
            state.save(INDEX).unwrap();
        }
    }
}
