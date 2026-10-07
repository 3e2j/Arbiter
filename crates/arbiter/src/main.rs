//! The CLI, or the editor with no args.

use std::{path::PathBuf, process::ExitCode, time::Instant};

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Writes every file on a disc under a directory.
    Unpack { disc: PathBuf, out: PathBuf },
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Unpack { disc, out } => {
            let start = Instant::now();
            match project::unpack(&disc, &out) {
                Ok(done) => {
                    println!(
                        "unpacked {} rev {}: {} files, {} MiB in {:.1?}",
                        done.id,
                        done.revision,
                        done.files,
                        done.bytes >> 20,
                        start.elapsed(),
                    );
                    ExitCode::SUCCESS
                }
                Err(err) => {
                    eprintln!("error: {err}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}
