use anyhow::{anyhow, Context, Result};
use clap::{App, Arg, ArgMatches};
use flib::*;

fn append(args: &ArgMatches, state: &mut State) -> Result<()> {
    let fname: String = args.value_of_t("file").context("No path..")?;

    do_append(state, &fname)
}

pub fn build() -> clap::App<'static> {
    let mut app = App::new("flog - the forgetful file log.")
        .version("2021")
        .author("Patrik Lundgren <patrik.lundgren.95@gmail.com>")
        .about("flog has a short but excellent memory, it remembers file(s) by name and \n");

    app = app.subcommand(
        App::new("append")
            .about("Append file-snapshot to history.")
            .arg(Arg::new("file").required(true).takes_value(true)),
    );

    //    app = app.subcommand(
    //        App::new("watch")
    //            .about("watch file, take snapshot when changed.")
    //            .arg(Arg::new("file").required(true).takes_value(true)),
    //    );

    app
}

fn dispatch(matches: &ArgMatches) {
    match matches.subcommand() {
        Some(("append", sargs)) => append(sargs, &mut load_index()),
        //        ("watch", sargs) => watch(sargs, &mut load_index()),
        _ => Err(anyhow!("Unrecognized command")),
    }
    .unwrap();
}

fn main() {
    let app = build();
    let matches = app.clone().try_get_matches();
    match matches {
        Ok(m) => dispatch(&m),
        Err(msg) => println!("{}", msg),
    };
}
