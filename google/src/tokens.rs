//! Where a device keeps its Google tokens. They are never synced over P2P: each device
//! signs in once.
//!
//! [`FileTokenStore`] writes a JSON file readable only by its owner. The plan wants the OS
//! keychain (libsecret, macOS Keychain, Android Keystore); that can sit behind the same
//! [`TokenStore`] trait later without touching the callers.

use std::io::Write;
use std::path::PathBuf;

use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: String,
    /// Epoch ms when the access token stops working.
    pub expires_at: i64,
}

pub trait TokenStore: Send + Sync {
    fn load(&self) -> Result<Option<Tokens>>;
    fn save(&self, tokens: &Tokens) -> Result<()>;
    /// Forgets the tokens (sign out, or Google revoked them).
    fn clear(&self) -> Result<()>;
}

pub struct FileTokenStore {
    path: PathBuf,
}

impl FileTokenStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        FileTokenStore { path: path.into() }
    }
}

impl TokenStore for FileTokenStore {
    fn load(&self) -> Result<Option<Tokens>> {
        match std::fs::read(&self.path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes).ok()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn save(&self, tokens: &Tokens) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // Write a sibling file, then rename, so a crash never leaves half a token.
        let tmp = self.path.with_extension("tmp");
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&tmp)?;
        file.write_all(&serde_json::to_vec(tokens)?)?;
        file.sync_all()?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    fn clear(&self) -> Result<()> {
        match std::fs::remove_file(&self.path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_loads_and_clears_with_private_permissions() {
        let dir = std::env::temp_dir().join(format!("utre-google-{}", std::process::id()));
        let store = FileTokenStore::new(dir.join("nested").join("token.json"));
        assert_eq!(store.load().unwrap(), None);
        let t = Tokens {
            access_token: "a".into(),
            refresh_token: "r".into(),
            expires_at: 5,
        };
        store.save(&t).unwrap();
        assert_eq!(store.load().unwrap(), Some(t));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join("nested").join("token.json"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(
                mode & 0o077,
                0,
                "token file must not be group/world readable"
            );
        }
        store.clear().unwrap();
        store.clear().unwrap();
        assert_eq!(store.load().unwrap(), None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
