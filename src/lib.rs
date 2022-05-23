use anyhow::{Context, Result};
use log::{Level, Log, Metadata, Record};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::prelude::*;

pub struct StdoutLog {
    pub level: Level,
}

impl Log for StdoutLog {
    fn enabled(&self, meta: &Metadata<'_>) -> bool {
        println!("log active?");
        meta.level() <= self.level
    }
    fn log(&self, record: &Record<'_>) {
        println!("trying to log");
        if self.enabled(record.metadata()) {
            println!("{}: {}", record.level(), record.args());
        }
    }
    fn flush(&self) {}
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Command {
    ECHOERR,
    ECHO,
    LIST,
    TRACK,
    SELECT,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Alias {
    SCRIPT(String),
    BASENAME,
    NAME(String),
}

#[derive(Serialize, Deserialize)]
pub struct Packet {
    pub command: Command,
    pub payload: Vec<u8>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum Action {
    SAVE,
    SCRIPT(String),
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

#[derive(Clone, Serialize, Deserialize)]
pub struct State {
    pub files: HashMap<String, Entry>,
}

pub const SOCK_PATH: &str = "/var/run/fwatchd.socket";

impl State {
    pub fn load(f: &str) -> Result<State> {
        let state = serde_json::from_reader::<std::fs::File, Self>(
            std::fs::File::open(f).context("Could not open file")?,
        )?;

        Ok(state)
    }

    pub fn save(&self, path: &str) -> Result<State> {
        let json = serde_json::to_string_pretty(&self).context("Failed to serialize state")?;
        std::fs::File::create(&path)
            .and_then(|mut f| f.write_all(json.as_bytes()))
            .context(format!("Failed to save file {path}"))?;

        Ok(self.clone())
    }

    pub fn new() -> State {
        State {
            files: HashMap::new(),
        }
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}
