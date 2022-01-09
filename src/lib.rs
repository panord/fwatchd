use anyhow::{Context, Result};
use crypto::digest::Digest;
use crypto::sha2;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::prelude::*;

pub const VARD: &str = "/var/run/flog";
pub const INDEX: &str = "/var/run/flog/index";
pub const INDEXD: &str = "/var/run/flog/index.d";

pub fn do_append(state: &mut State, fname: &str) -> Result<()> {
    let mut hasher = sha2::Sha256::new();
    let mut contents = String::new();
    let fpath = std::path::Path::new(&fname);
    let mut file = std::fs::File::open(&fpath)?;

    file.read_to_string(&mut contents)?;
    hasher.input_str(&contents);
    let target = format!(
        "{}/{}-{}",
        INDEXD,
        fpath.display().to_string(),
        hasher.result_str()
    );
    println!("{}", &target);
    std::fs::create_dir_all(&std::path::Path::new(&target).parent().unwrap())?;
    std::fs::copy(&fpath, &target).expect("Failed to save file version");

    state
        .files
        .entry(fpath.display().to_string())
        .or_insert(HashMap::new())
        .insert(hasher.result_str(), target);

    state.save(INDEX)?;
    Ok(())
}

pub fn load_index() -> State {
    State::load(INDEX).expect("Failed loading or creating config")
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct State {
    pub files: HashMap<String, HashMap<String, String>>,
}

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
            .and_then(|mut f| f.write_all(&json.as_bytes()))
            .context("Failed to save file.")?;

        Ok(self.clone())
    }

    pub fn new() -> State {
        State {
            files: HashMap::new(),
        }
    }
}
