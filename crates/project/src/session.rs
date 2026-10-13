//! The editor's session, `.arbiter/session`: what it had open and where, as
//! the front end wrote it. Its contents are the front end's own.

use std::{fs, io};

use crate::{Project, ProjectError, base};

/// Under the project root.
const PATH: &str = ".arbiter/session";

impl Project {
    /// The session as last written, or `None` when there's none yet.
    ///
    /// # Errors
    ///
    /// If it's there but can't be read.
    pub fn read_session(&self) -> Result<Option<String>, ProjectError> {
        let path = self.root.join(PATH);
        match fs::read_to_string(&path) {
            Ok(text) => Ok(Some(text)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(base::io_err(&path)(err).into()),
        }
    }

    /// Replaces the session with `text`, never leaving half of it.
    ///
    /// # Errors
    ///
    /// If `.arbiter/` can't be made or the session can't be written.
    pub fn write_session(&self, text: &str) -> Result<(), ProjectError> {
        let path = self.root.join(PATH);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(base::io_err(dir))?;
        }
        base::write_atomic(&path, text.as_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::Project;

    fn project(root: &std::path::Path) -> Project {
        std::fs::write(root.join(crate::config::FILE), "kind = \"game\"\n").unwrap();
        Project::open(root).unwrap()
    }

    #[test]
    fn no_session_reads_as_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(project(dir.path()).read_session().unwrap(), None);
    }

    #[test]
    fn a_session_reads_back_as_written() {
        let dir = tempfile::tempdir().unwrap();
        let project = project(dir.path());
        project.write_session("first").unwrap();
        project.write_session("second").unwrap();
        assert_eq!(project.read_session().unwrap().as_deref(), Some("second"));
        assert!(!dir.path().join(".arbiter/session.tmp").exists());
    }
}
