//! The CLI, or the editor with no args.

mod logging;
mod prompt;
mod unpack;

use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::{Parser, Subcommand};
use project::{Log, Project};
use unpack::unpack;

#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Opens the editor when none is given.
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Creates a mod project, runs `git init`, and unpacks each disc into its base.
    New {
        dir: PathBuf,
        discs: Vec<PathBuf>,
        /// Replaces the directory if it isn't empty.
        #[arg(long)]
        force: bool,
    },
    /// Unpacks each disc into an existing project's base. A new edition is
    /// added beside the others. Another revision of an edition replaces the
    /// old one, if `changes/` can still apply to it.
    Unpack {
        #[arg(required = true)]
        discs: Vec<PathBuf>,
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error(transparent)]
    Project(#[from] project::Error),

    #[error(transparent)]
    App(#[from] app::Error),

    #[error("{0} was given more than once, and no one picked which to keep")]
    Duplicate(String),
}

fn main() -> ExitCode {
    let command = Cli::parse().command;
    let log = logging::init(command.is_none());
    let result = match command {
        None => app::run().map_err(Error::from),
        Some(Command::New { dir, discs, force }) => new(&dir, &discs, force, &log),
        Some(Command::Unpack { discs, project }) => Project::open(&project)
            .map_err(Error::from)
            .and_then(|mut p| {
                logging::attach(&log, &p.root);
                unpack(&mut p, &discs)
            }),
    };
    if let Err(err) = log.flush() {
        eprintln!("warning: the log's last lines weren't written: {err}");
    }
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            match err {
                Error::Project(project::Error::NotEmpty(_)) => {
                    eprintln!("help: pass --force to replace it (will wipe all project files!)");
                }
                Error::Duplicate(_) => eprintln!("help: pass only one disc per edition"),
                Error::Project(_) | Error::App(_) => {}
            }
            ExitCode::FAILURE
        }
    }
}

/// Logs into the project once it's finished, writing out everything before.
fn new(dir: &Path, discs: &[PathBuf], force: bool, log: &Log) -> Result<(), Error> {
    let mut draft = Project::create(dir, force)?;
    unpack(&mut draft.project, discs)?;
    let project = draft.finish()?;
    logging::attach(log, &project.root);
    println!("created mod project at {}", dir.display());
    Ok(())
}
