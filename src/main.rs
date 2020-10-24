#![warn(clippy::pedantic)]

use clap::{load_yaml, App};

mod commands;
mod messages;

use messages::{error, info, warn};

fn main() {
    let yaml = load_yaml!("data/cli-en.yml");
    let matches = App::from_yaml(yaml)
        .version(env!("CARGO_PKG_VERSION"))
        .get_matches();

    if let Some(matches) = matches.subcommand_matches("debug") {
        commands::debug(matches);
    }
}