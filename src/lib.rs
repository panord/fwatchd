use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::prelude::*;

#[derive(Debug, Serialize, Deserialize)]
pub enum Command {
    ECHOERR,
    ECHO,
    LIST,
    TRACK,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    pub files: HashMap<String, Entry>,
}

pub const SOCK_PATH: &str = "/var/run/flogd.socket";

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
            .context("Failed to save file.")?;

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
