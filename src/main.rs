use anyhow::{anyhow, Context, Result};
use clap::{value_t, App, Arg, ArgMatches, SubCommand};
use crypto::digest::Digest;
use crypto::sha2;
use serde::{Deserialize, Serialize};
use std::io::prelude::*;

#[derive(Debug, Serialize, Deserialize, Clone)]
struct FSnapShot {
    fpath: String,
    hash: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct State {
    files: Vec<FSnapShot>,
}

impl State {
    fn load(f: &str) -> Result<State> {
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
        State { files: vec![] }
    }
}

fn init(_args: &ArgMatches) -> Result<()> {
    std::fs::create_dir(".flog")?;
    State::new().save(".flog/index")?;
    Ok(())
}

fn append(args: &ArgMatches, state: &mut State) -> Result<()> {
    let mut hasher = sha2::Sha256::new();
    let mut contents = String::new();
    let fpath = value_t!(args.value_of("file"), String).context("No path..")?;
    let mut file = std::fs::File::open(&fpath)?;
    file.read_to_string(&mut contents)?;

    hasher.input_str(&contents);
    state.files.push(FSnapShot {
        fpath: fpath,
        hash: hasher.result_str(),
    });
    state.save(".flog/index")?;
    Ok(())
}

pub fn build() -> clap::App<'static, 'static> {
    let mut app = App::new("flog - the forgetful file log.")
        .version("2021")
        .author("Patrik Lundgren <patrik.lundgren@outlook.com>")
        .about("flog has a short but excellent memory, it remembers file(s) by name and \n");

    app = app.subcommand(SubCommand::with_name("init").about("Initialize .flog directory."));
    app = app.subcommand(
        SubCommand::with_name("append")
            .about("Append file-snapshot to history.")
            .arg(Arg::with_name("file").required(true).takes_value(true)),
    );

    app
}

fn load_index() -> State {
    State::load(".flog/index")
        //.or_else(|_| State::new().save(".flog/index"))
        .expect("Failed loading or creating config")
}

fn dispatch(matches: &ArgMatches) {
    match matches.subcommand() {
        ("init", Some(sargs)) => init(sargs),
        ("append", Some(sargs)) => append(sargs, &mut load_index()),
        _ => Err(anyhow!("Unrecognized command")),
    }
    .unwrap_or_else(|e| {
        println!("{}", e);
    });
}

fn main() {
    let app = build();
    let matches = app.clone().get_matches_safe();
    match matches {
        Ok(m) => dispatch(&m),
        Err(msg) => println!("{}", msg),
    };
}
