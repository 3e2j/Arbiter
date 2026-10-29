//! The CLI, or the editor with no subcommand.

mod logging;
mod prompt;
mod unpack;

use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::{Parser, Subcommand};
use project::{LOG, Project};
use unpack::unpack;

#[derive(Parser)]
#[command(version, args_conflicts_with_subcommands = true)]
struct Cli {
    /// Opens the editor when none is given.
    #[command(subcommand)]
    command: Option<Command>,
    /// A project for the editor to open.
    project: Option<PathBuf>,
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
    Project(#[from] project::ProjectError),

    #[error(transparent)]
    App(#[from] app::AppError),

    #[error("{0} was given more than once, and no one picked which to keep")]
    Duplicate(String),
}

fn main() -> ExitCode {
    let Cli { command, project } = Cli::parse();
    logging::init();
    let result = match command {
        None => project
            .as_deref()
            .map(Project::open)
            .transpose()
            .map_err(Error::from)
            .and_then(|p| app::run(p).map_err(Error::from)),
        Some(Command::New { dir, discs, force }) => new(&dir, &discs, force),
        Some(Command::Unpack { discs, project }) => Project::open(&project)
            .map_err(Error::from)
            .and_then(|mut p| {
                logging::attach(&p.root);
                unpack(&mut p, &discs)
            }),
    };
    if let Err(err) = LOG.flush() {
        eprintln!("warning: the log's last lines weren't written: {err}");
    }
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            match err {
                Error::Project(project::ProjectError::NotEmpty(_)) => {
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
fn new(dir: &Path, discs: &[PathBuf], force: bool) -> Result<(), Error> {
    let mut draft = Project::create(dir, force)?;
    unpack(&mut draft.project, discs)?;
    let project = draft.finish()?;
    logging::attach(&project.root);
    println!("created mod project at {}", dir.display());
    Ok(())
}
