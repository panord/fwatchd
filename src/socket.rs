use log::{Level, Log, Metadata, Record};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub struct StdoutLog {
    pub level: Level,
}

impl Log for StdoutLog {
    fn enabled(&self, meta: &Metadata<'_>) -> bool {
        meta.level() <= self.level
    }
    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            println!("{}: {}", record.level(), record.args());
        }
    }
    fn flush(&self) {}
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Command {
    Echoerr,
    Echo,
    List,
    Track,
    Select,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Alias {
    Script(String),
    Basename,
    Name(String),
}

#[derive(Serialize, Deserialize)]
pub struct Packet {
    pub command: Command,
    pub payload: Vec<u8>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum Action {
    Save,
    Script(String),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Track {
    pub fpath: String,
    pub alias: Alias,
    pub action: Action,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    // hash --> filename
    pub snapshots: HashMap<String, (String, String)>,
    pub action: Action,
    pub alias: Alias,
}

pub const SOCK_PATH: &str = "/var/run/fwatchd.socket";
