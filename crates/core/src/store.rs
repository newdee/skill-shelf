use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::Result;

/// Content-addressable object store on the filesystem.
///
/// Every blob (file content), tree (directory manifest) and commit object is
/// stored under `objects/<hh>/<hash>` keyed by the SHA-256 of its bytes, so
/// identical content is stored exactly once — unchanged files are shared
/// across commits for free.
pub struct ObjectStore {
    root: PathBuf,
}

impl ObjectStore {
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    pub fn hash(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }

    fn path_for(&self, hash: &str) -> PathBuf {
        self.root.join(&hash[0..2]).join(hash)
    }

    /// Store `data`, returning its content hash. Idempotent: writing the same
    /// bytes twice is a no-op the second time.
    pub fn put(&self, data: &[u8]) -> Result<String> {
        let hash = Self::hash(data);
        let path = self.path_for(&hash);
        if !path.exists() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&path, data)?;
        }
        Ok(hash)
    }

    pub fn get(&self, hash: &str) -> Result<Vec<u8>> {
        Ok(fs::read(self.path_for(hash))?)
    }

    #[allow(dead_code)] // store primitive; used by later phases (import/gc)
    pub fn exists(&self, hash: &str) -> bool {
        self.path_for(hash).exists()
    }
}
