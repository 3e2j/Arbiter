//! Unpacking discs into a project's base, for `new` and `unpack`.

use std::{
    io::{self, IsTerminal},
    path::{Path, PathBuf},
    time::Instant,
};

use diag::Severity;
use project::{Game, Peeked, Project, Recorded, Staged, Verdict, hash::Hash};

use crate::{
    Error,
    prompt::{choose, confirm},
};

/// A disc to unpack, read and hashed before any is.
struct Given<'a> {
    path: &'a Path,
    peeked: Peeked,
    hash: Hash,
    verdict: Verdict,
}

/// Reads and hashes every disc first, so the target's game, which disc to keep
/// per edition and whether to take a modified one are settled before
/// unpacking. Then stages and commits one disc at a time.
pub fn unpack(project: &mut Project, discs: &[PathBuf]) -> Result<(), Error> {
    let peeked = discs
        .iter()
        .map(|disc| Ok((disc.as_path(), Project::peek(disc)?)))
        .collect::<Result<Vec<_>, project::Error>>()?;
    if !io::stdin().is_terminal()
        && let Some((_, p)) = peeked
            .iter()
            .find(|(_, p)| peeked.iter().filter(|(_, q)| q.id == p.id).count() > 1)
    {
        return Err(Error::Duplicate(p.id.clone()));
    }
    let game = match project.config.target_game() {
        Some(game) => Some(game),
        None => pick_game(&peeked),
    };

    let start = Instant::now();
    let pairs: Vec<_> = peeked.iter().map(|(disc, p)| (*disc, p)).collect();
    let verified = Project::verify(&pairs)?;
    println!("hashed {} disc(s) in {:.1?}", pairs.len(), start.elapsed());
    let given: Vec<_> = peeked
        .into_iter()
        .zip(verified)
        .map(|((path, peeked), (hash, verdict))| Given {
            path,
            peeked,
            hash,
            verdict,
        })
        .collect();

    for (disc, picked) in pick_discs(&given)? {
        if !retail(disc, picked) {
            println!("skipped {}", disc.path.display());
            continue;
        }
        let staged = stage(project, disc)?;
        let target = is_target(project, &staged, game);
        commit(project, staged, target)?;
    }
    Ok(())
}

/// `None` if a known game can't be picked. Then every known edition is a reference.
fn pick_game(peeked: &[(&Path, Peeked)]) -> Option<Game> {
    let mut games: Vec<Game> = Vec::new();
    for game in peeked.iter().filter_map(|(_, p)| p.game) {
        if !games.contains(&game) {
            games.push(game);
        }
    }
    if games.len() < 2 {
        return games.first().copied();
    }
    let titles: Vec<_> = games.iter().map(|g| g.title()).collect();
    let picked = choose(
        "these discs are from different games, which one is the target?",
        &titles,
    )
    .and_then(|i| games.get(i).copied());
    if picked.is_none() {
        println!("note: no game picked, so every known edition is a reference");
    }
    picked
}

/// One disc per edition, in the order given, and whether the user picked it
/// from others of its edition.
fn pick_discs<'a, 'b>(given: &'b [Given<'a>]) -> Result<Vec<(&'b Given<'a>, bool)>, Error> {
    let mut kept = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for disc in given {
        let id = disc.peeked.id.as_str();
        if seen.contains(&id) {
            continue;
        }
        seen.push(id);
        let same: Vec<_> = given.iter().filter(|d| d.peeked.id == id).collect();
        if let [only] = same[..] {
            kept.push((only, false));
            continue;
        }

        let options: Vec<_> = same
            .iter()
            .map(|d| {
                let verdict = match d.verdict {
                    Verdict::Retail => "retail",
                    Verdict::Mismatch { .. } => "not retail",
                    Verdict::Uncatalogued => "unchecked",
                };
                format!(
                    "{} (rev {}, {verdict})",
                    d.path.display(),
                    d.peeked.revision
                )
            })
            .collect();
        let options: Vec<_> = options.iter().map(String::as_str).collect();
        let picked = choose(
            &format!(
                "{id} was given {} times, which one should be kept?",
                same.len()
            ),
            &options,
        )
        .and_then(|i| same.get(i).copied())
        .ok_or_else(|| Error::Duplicate(id.to_owned()))?;
        kept.push((picked, true));
    }
    Ok(kept)
}

/// Warns about a disc that isn't a clean dump. Whether to unpack it anyway,
/// asked unless it was picked over others already knowing.
fn retail(disc: &Given, picked: bool) -> bool {
    let Given { peeked: p, .. } = disc;
    match disc.verdict {
        Verdict::Retail => true,
        Verdict::Mismatch { expected } => {
            eprintln!(
                "warning: {} rev {} isn't a clean retail disc, expected xxh3:{expected:032x}",
                p.id, p.revision
            );
            eprintln!("note: a scrubbed, trimmed or modified disc can unpack different files");
            picked || confirm("unpack anyway?")
        }
        Verdict::Uncatalogued => {
            println!(
                "note: no clean dump of {} rev {} is known, so it can't be checked",
                p.id, p.revision
            );
            true
        }
    }
}

/// An edition already in the project keeps its place. A known one is a target
/// if it's the target's game, an unknown one if the user says so.
fn is_target(project: &Project, staged: &Staged, game: Option<Game>) -> bool {
    if project.config.editions.contains_key(&staged.id) {
        return project.config.is_target(&staged.id);
    }
    if let Some(e) = staged.edition {
        return Some(e.game) == game;
    }
    let guess = staged
        .guess
        .map_or_else(String::new, |g| format!(", it looks like {}", g.title()));
    confirm(&format!(
        "{} is unknown{guess}. Add it to the target?",
        staged.id
    ))
}

/// Unpacks and reports a disc.
fn stage(project: &Project, disc: &Given) -> Result<Staged, project::Error> {
    let start = Instant::now();
    let staged = project.stage(disc.path, disc.hash)?;
    println!(
        "unpacked {} rev {}: {} files, {} MiB in {:.1?}, disc {}",
        staged.id,
        staged.revision,
        staged.manifest.files.len(),
        staged.bytes >> 20,
        start.elapsed(),
        staged.disc_hash,
    );
    let title = staged.edition.map_or("unknown game", |e| e.game.title());
    println!(
        "{title}: {} region, {} release",
        known(staged.region),
        known(staged.country)
    );
    if staged.edition.is_none() {
        println!("note: {} isn't a known edition, values show raw", staged.id);
    }
    Ok(staged)
}

/// Checks a revision switch against `changes/`, then commits.
fn commit(project: &mut Project, staged: Staged, target: bool) -> Result<(), project::Error> {
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
    if let Recorded::Changed { was } = project.commit(staged, target)? {
        println!("warning: {id} differs from the base this project was made against ({was})");
    }
    if target {
        println!("{id} is a target");
    } else {
        println!("{id} is a reference: read-only, never built");
    }
    Ok(())
}

fn known(value: Option<impl std::fmt::Debug>) -> String {
    value.map_or_else(|| "unknown".to_owned(), |v| format!("{v:?}"))
}
