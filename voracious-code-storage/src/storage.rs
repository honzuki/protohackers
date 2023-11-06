use std::{
    collections::{BTreeSet, HashMap},
    hash::Hash,
};

use dashmap::DashMap;

#[derive(Debug, Default)]
struct TempFile {
    revisions: Vec<async_tempfile::TempFile>,
    hashes: HashMap<Vec<u8>, u64>,
}

impl TempFile {
    fn insert(&mut self, file: async_tempfile::TempFile, hash: Vec<u8>) -> u64 {
        // no need to store duplicate of existing files
        if let Some(revision) = self.hashes.get(&hash) {
            return *revision;
        }

        self.revisions.push(file);
        let revision = self.revisions.len() as u64;

        self.hashes.insert(hash, revision);

        revision
    }

    async fn get(&self, revision: u64) -> Option<async_tempfile::TempFile> {
        match self.revisions.get((revision - 1) as usize) {
            Some(revision) => {
                Some(revision.try_clone().await.expect(
                    "we only ever read files in the filesystem, clone should always succedd",
                ))
            }
            None => None,
        }
    }

    fn get_last_revision(&self) -> u64 {
        self.revisions.len() as u64
    }
}

// Represents an item in a dir
#[derive(Debug, Eq)]
enum DirItemStab {
    File(String),
    Dir(String),
}

#[derive(Debug, Default)]
pub struct TempFileSystem {
    files: DashMap<String, TempFile>,
    dirs: DashMap<String, BTreeSet<DirItemStab>>,
}

#[derive(thiserror::Error, Debug)]
pub enum GetFileErr {
    #[error("no such file")]
    FileNotFound,

    #[error("no such revision")]
    RevisionNotFound,
}

#[derive(Debug)]
pub enum ListResult {
    Dir(String),
    File { name: String, last_revision: u64 },
}

impl TempFileSystem {
    /// inserts a new file into the filesystem
    /// returns the revision number
    pub fn insert(&self, filepath: String, file: async_tempfile::TempFile, hash: Vec<u8>) -> u64 {
        // insert the file
        let mut file_stab = self.files.entry(filepath.clone()).or_default();
        let revision = file_stab.insert(file, hash);

        // update all dirs
        let mut path = "/".to_string();
        // skip the starting '/'
        let mut parts = filepath[1..].split('/');
        let filename = parts.next_back().expect("file name can't be empty");
        for dirname in parts {
            self.dirs
                .entry(path.clone())
                .or_default()
                .insert(DirItemStab::Dir(dirname.to_string()));

            path += dirname;
            path += "/";
        }

        self.dirs
            .entry(path.clone())
            .or_default()
            .insert(DirItemStab::File(filename.into()));

        revision
    }

    /// if the file exists, will return a clone of the tempfile
    /// that can then be used to read the file content.
    /// the function trust and rely on the caller to not write to the file, only read it.
    ///
    /// returns an error if the correct revision of the file can't be found
    pub async fn get(
        &self,
        name: &str,
        revision: Option<u64>,
    ) -> Result<async_tempfile::TempFile, GetFileErr> {
        let Some(file) = self.files.get(name) else {
            return Err(GetFileErr::FileNotFound);
        };

        match revision {
            Some(revision) => Ok(file
                .get(revision)
                .await
                .ok_or(GetFileErr::RevisionNotFound)?),
            None => Ok(file.get(file.get_last_revision()).await.unwrap()),
        }
    }

    // returns the list of children of a given directory
    pub fn list(&self, dir_path: &str) -> Vec<ListResult> {
        let Some(dir) = self.dirs.get(dir_path) else {
            return vec![];
        };

        dir.iter()
            .map(|stab| match stab {
                DirItemStab::Dir(name) => ListResult::Dir(name.clone()),
                DirItemStab::File(name) => {
                    let last_revision = self
                        .files
                        .get(&format!("{}{}", dir_path, name))
                        .unwrap()
                        .get_last_revision();

                    ListResult::File {
                        name: name.clone(),
                        last_revision,
                    }
                }
            })
            .collect()
    }
}

// necessary traits impl for the list of dirs to be ordered
impl PartialEq for DirItemStab {
    fn eq(&self, other: &Self) -> bool {
        self.name().eq(other.name())
    }
}

impl Ord for DirItemStab {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.name().cmp(other.name())
    }
}

impl PartialOrd for DirItemStab {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Hash for DirItemStab {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name().hash(state)
    }
}

impl DirItemStab {
    fn name(&self) -> &str {
        match self {
            Self::Dir(name) => name,
            Self::File(name) => name,
        }
    }
}
