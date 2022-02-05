use anyhow::{anyhow, Context, Result};
use clap::{App, Arg, ArgMatches};
use flib::*;

fn append(args: &ArgMatches, state: &mut State) -> Result<()> {
    let fname: String = args.value_of_t("file").context("No path..")?;

    do_append(state, &fname)
}

fn list(args: &ArgMatches, state: &mut State) -> Result<()> {
    let fname: Option<String> = args.value_of_t("file").ok();
    if fname.is_some() {
        let h = state
            .files
            .get(&fname.unwrap())
            .context("Found no such tracked file")?;
        println!("{:?}", h);
    } else {
        println!("{:?}", state.files);
    }
    Ok(())
}

pub fn build() -> clap::App<'static> {
    let mut app = App::new("flog - the forgetful file log.")
        .version("2021")
        .author("Patrik Lundgren <patrik.lundgren.95@gmail.com>")
        .about("flog has a short but excellent memory, it remembers file(s) by name and \n");

    app = app.subcommand(
        App::new("track")
            .about("track file, take snapshot when changed.")
            .arg(Arg::new("file").required(true).takes_value(true)),
    );

    app = app.subcommand(
        App::new("list")
            .about("list available file snapshots")
            .arg(Arg::new("file").takes_value(true)),
    );

    app
}

fn dispatch(app: &mut App, matches: &ArgMatches) {
    match matches.subcommand() {
        Some(("track", sargs)) => append(sargs, &mut load_index()),
        Some(("list", sargs)) => list(sargs, &mut load_index()),
        None => {
            println!("{}", app.render_usage());
            Ok(())
        }
        _ => Err(anyhow!("Unrecognized command")),
    }
    .unwrap();
}

fn main() {
    let mut app = build();
    let matches = app.clone().try_get_matches();
    match matches {
        Ok(m) => dispatch(&mut app, &m),
        Err(msg) => println!("{}", msg),
    };
}
