//! Every open file, read and decoded once and kept by its path, for each tab
//! that shows it. A file no tab shows any more is dropped.

use std::collections::HashMap;
use std::io;
use std::sync::mpsc::{Receiver, TryRecvError};

use formats::games::tp::Version;
use project::Project;

use crate::fs::{FileLoader, Loaded};
use crate::panels::Format;

pub struct Documents {
    /// Unless its thread couldn't start.
    loader: Option<FileLoader>,
    /// Keyed by the path relative to the project's root.
    open: HashMap<String, Document>,
}

pub enum Document {
    /// Asked of the [`FileLoader`], with the TP version whose base it's in.
    Loading(Receiver<Loaded>, Option<Version>),
    /// Why it couldn't be read.
    Failed(String),
    /// Bytes no format claims.
    Raw(Vec<u8>),
    Decoded(Format),
}

impl Documents {
    pub fn new(loader: Option<FileLoader>) -> Self {
        Self {
            loader,
            open: HashMap::new(),
        }
    }

    /// `path`, relative to `project`'s root, asked of the loader the first
    /// time. `None` until there's a project and loader to ask.
    pub fn open(&mut self, path: &str, project: Option<&Project>) -> Option<&mut Document> {
        if !self.open.contains_key(path) {
            let (project, loader) = (project?, self.loader.as_ref()?);
            let loading = loader.ask(project.root.join(path));
            let document = Document::Loading(loading, version(project, path));
            self.open.insert(path.to_owned(), document);
        }
        self.open.get_mut(path)
    }

    pub fn get(&self, path: &str) -> Option<&Document> {
        self.open.get(path)
    }

    /// Takes each file read since last asked, decoding it if a format claims
    /// it.
    pub fn poll(&mut self) {
        for (path, document) in &mut self.open {
            let Document::Loading(loading, version) = document else {
                continue;
            };
            let read = match loading.try_recv() {
                Ok(read) => read,
                Err(TryRecvError::Empty) => continue,
                Err(TryRecvError::Disconnected) => Err(io::Error::other("the loader stopped")),
            };
            *document = match read {
                Ok(bytes) => match Format::open(path, &bytes, *version) {
                    Some(format) => Document::Decoded(format),
                    None => Document::Raw(bytes),
                },
                Err(err) => Document::Failed(format!("Can't read {path}: {err}")),
            };
        }
    }

    /// Drops every file whose path isn't in `live`.
    pub fn sweep<'a>(&mut self, live: impl Iterator<Item = &'a str>) {
        let live: Vec<_> = live.collect();
        self.open.retain(|path, _| live.contains(&path.as_str()));
    }
}

/// The TP version whose base `path` is in, relative to `project`'s root.
fn version(project: &Project, path: &str) -> Option<Version> {
    let (id, edition) = project.edition_of(path)?;
    Version::from_disc(id, edition.revision)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sweep_keeps_only_what_a_tab_shows() {
        let mut documents = Documents::new(None);
        for path in ["a.bin", "b.bin"] {
            documents
                .open
                .insert(path.to_owned(), Document::Raw(Vec::new()));
        }
        documents.sweep(["b.bin"].into_iter());
        assert!(documents.open("b.bin", None).is_some());
        assert!(documents.open("a.bin", None).is_none());
    }
}
