use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Arg, Command, value_parser};

#[derive(Debug)]
pub enum CliCommand {
    Init {
        path: PathBuf,
    },
    Add {
        inputs: Vec<PathBuf>,
        repo: Option<PathBuf>,
        recursive: bool,
    },
}

pub fn parse() -> Result<CliCommand> {
    let matches = Command::new("paperout")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Minimal PDF-to-Markdown paper workflow")
        .subcommand(
            Command::new("init").arg(
                Arg::new("path")
                    .value_name("PATH")
                    .value_parser(value_parser!(PathBuf)),
            ),
        )
        .subcommand(
            Command::new("add")
                .arg(
                    Arg::new("inputs")
                        .required(true)
                        .num_args(1..)
                        .value_name("INPUT")
                        .value_parser(value_parser!(PathBuf)),
                )
                .arg(
                    Arg::new("repo")
                        .long("repo")
                        .value_name("PATH")
                        .value_parser(value_parser!(PathBuf)),
                )
                .arg(
                    Arg::new("recursive")
                        .short('r')
                        .long("recursive")
                        .action(clap::ArgAction::SetTrue),
                ),
        )
        .get_matches();

    match matches.subcommand() {
        Some(("init", sub_matches)) => {
            let path = sub_matches
                .get_one::<PathBuf>("path")
                .cloned()
                .unwrap_or(std::env::current_dir().context("failed to get current directory")?);
            Ok(CliCommand::Init { path })
        }
        Some(("add", sub_matches)) => Ok(CliCommand::Add {
            inputs: sub_matches
                .get_many::<PathBuf>("inputs")
                .context("missing PDF inputs")?
                .cloned()
                .collect(),
            repo: sub_matches.get_one::<PathBuf>("repo").cloned(),
            recursive: sub_matches.get_flag("recursive"),
        }),
        Some((name, _)) => bail!("unsupported command: {name}"),
        None => bail!("no command provided"),
    }
}
