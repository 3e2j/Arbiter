//! The CLI, or the editor with no args.

use std::{
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    time::Instant,
};

use clap::{Parser, Subcommand};
use diag::Severity;
use project::{Project, Recorded};

#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
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
    /// Unpacks a disc into an existing project's base again.
    /// Another revision of an edition replaces the old one,
    /// if `changes/` can still apply to it.
    Unpack {
        disc: PathBuf,
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::New { dir, discs, force } => new(&dir, &discs, force),
        Command::Unpack { disc, project } => {
            Project::open(&project).and_then(|mut p| unpack(&mut p, &disc))
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            if let project::Error::NotEmpty(_) = err {
                eprintln!("help: pass --force to replace it (will wipe all project files!)");
            }
            ExitCode::FAILURE
        }
    }
}

fn new(dir: &Path, discs: &[PathBuf], force: bool) -> Result<(), project::Error> {
    let mut draft = Project::create(dir, force)?;
    for disc in discs {
        unpack(&mut draft.project, disc)?;
    }
    draft.finish()?;
    println!("created mod project at {}", dir.display());
    Ok(())
}

fn unpack(project: &mut Project, disc: &Path) -> Result<(), project::Error> {
    let start = Instant::now();
    let staged = project.stage(disc)?;
    println!(
        "unpacked {} rev {}: {} files, {} MiB in {:.1?}",
        staged.id,
        staged.revision,
        staged.manifest.files.len(),
        staged.bytes >> 20,
        start.elapsed(),
    );

    if let Recorded::Switched { from } = staged.outcome {
        println!(
            "switching {} from rev {from} to rev {}",
            staged.id, staged.revision
        );
        let problems = project.switch_check(&staged);
        if !problems.items.is_empty() {
            for d in &problems.items {
                let severity = match d.severity() {
                    Severity::Warning => "warning",
                    Severity::Error => "error",
                };
                eprintln!("{severity}[{}]: {}", d.code.id, d.message);
            }
            eprintln!(
                "warning: {} change(s) won't apply cleanly to rev {}",
                problems.items.len(),
                staged.revision
            );
            if !confirm("switch anyway?") {
                println!("kept rev {from}");
                staged.discard()?;
                return Ok(());
            }
        }
    }

    let id = staged.id.clone();
    if let Recorded::Changed { was } = project.commit(staged)? {
        println!("warning: {id} differs from the base this project was made against ({was})");
    }
    Ok(())
}

/// No, unless someone at a terminal answers yes.
fn confirm(question: &str) -> bool {
    if !io::stdin().is_terminal() {
        return false;
    }
    eprint!("{question} [y/N] ");
    let _ = io::stderr().flush();
    let mut answer = String::new();
    io::stdin().read_line(&mut answer).is_ok() && answer.trim().eq_ignore_ascii_case("y")
}
