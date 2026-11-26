//! Account secrets encrypted with XChaCha20-Poly1305 under a key kept in
//! its own file, outside the database.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop};

const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;
const VERSION: u8 = 1;

/// The 32-byte key that encrypts account secrets. Wiped from memory on drop.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct SecretKey([u8; KEY_LEN]);

#[derive(Debug, Error)]
pub enum KeyFileError {
    #[error("key_file_io: {path}")]
    Io { path: PathBuf, source: io::Error },
    #[error("key_file_wrong_length: {path} ({len} bytes)")]
    WrongLength { path: PathBuf, len: u64 },
    #[error("key_file_too_open: {path} (mode {mode:o})")]
    TooOpen { path: PathBuf, mode: u32 },
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum SecretError {
    #[error("secret_undecryptable")]
    Undecryptable,
}

/// Reads the key at `path`, or creates it (mode `0600`) when the file
/// does not exist. An existing file of the wrong length, or readable by
/// group or others, is refused and left untouched.
pub fn load_or_create_key(path: &Path) -> Result<SecretKey, KeyFileError> {
    let io_err = |source| KeyFileError::Io {
        path: path.to_owned(),
        source,
    };
    match fs::metadata(path) {
        Ok(meta) => {
            if meta.len() != KEY_LEN as u64 {
                return Err(KeyFileError::WrongLength {
                    path: path.to_owned(),
                    len: meta.len(),
                });
            }
            let mode = meta.permissions().mode() & 0o777;
            if mode & 0o077 != 0 {
                return Err(KeyFileError::TooOpen {
                    path: path.to_owned(),
                    mode,
                });
            }
            let mut bytes = fs::read(path).map_err(io_err)?;
            let key =
                bytes
                    .as_slice()
                    .try_into()
                    .map(SecretKey)
                    .map_err(|_| KeyFileError::WrongLength {
                        path: path.to_owned(),
                        len: bytes.len() as u64,
                    });
            bytes.zeroize();
            key
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => create_key(path).map_err(io_err),
        Err(err) => Err(io_err(err)),
    }
}

fn create_key(path: &Path) -> io::Result<SecretKey> {
    let mut key = SecretKey([0; KEY_LEN]);
    getrandom::fill(&mut key.0).map_err(io::Error::other)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(&key.0)?;
    file.sync_all()?;
    tracing::warn!(path = %path.display(), "generated a new secret key");
    Ok(key)
}

/// Encrypts `plain` for one account: `[version][nonce][ciphertext]`, with
/// the account id as associated data.
pub fn encrypt(key: &SecretKey, account_id: i64, plain: &[u8]) -> Vec<u8> {
    let mut nonce = [0u8; NONCE_LEN];
    getrandom::fill(&mut nonce).expect("the OS random source is available");
    let aad = account_id.to_le_bytes();
    let ciphertext = XChaCha20Poly1305::new((&key.0).into())
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plain,
                aad: &aad,
            },
        )
        .expect("encryption of an in-memory buffer cannot fail");
    let mut stored = Vec::with_capacity(1 + NONCE_LEN + ciphertext.len());
    stored.push(VERSION);
    stored.extend_from_slice(&nonce);
    stored.extend_from_slice(&ciphertext);
    stored
}

/// Decrypts a value written by [`encrypt`] for the same account and key.
pub fn decrypt(key: &SecretKey, account_id: i64, stored: &[u8]) -> Result<Vec<u8>, SecretError> {
    if stored.len() <= 1 + NONCE_LEN || stored[0] != VERSION {
        return Err(SecretError::Undecryptable);
    }
    let (nonce, ciphertext) = stored[1..].split_at(NONCE_LEN);
    let aad = account_id.to_le_bytes();
    XChaCha20Poly1305::new((&key.0).into())
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| SecretError::Undecryptable)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use tempfile::TempDir;

    use super::*;

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    fn write_key(dir: &TempDir, name: &str, len: usize, mode: u32) -> PathBuf {
        let path = dir.path().join(name);
        fs::write(&path, vec![7u8; len]).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
        path
    }

    fn key(dir: &TempDir, name: &str) -> SecretKey {
        load_or_create_key(&dir.path().join(name)).unwrap()
    }

    #[test]
    fn missing_key_file_is_created_0600() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("secret.key");
        load_or_create_key(&path).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().len(), 32);
        assert_eq!(mode(&path), 0o600);
    }

    #[test]
    fn existing_key_is_reused() {
        let dir = TempDir::new().unwrap();
        let first = key(&dir, "secret.key");
        let second = key(&dir, "secret.key");
        let stored = encrypt(&first, 1, b"token");
        assert_eq!(decrypt(&second, 1, &stored).unwrap(), b"token");
    }

    #[test]
    fn short_key_is_refused() {
        let dir = TempDir::new().unwrap();
        for len in [0usize, 31, 33] {
            let path = write_key(&dir, &format!("k{len}"), len, 0o600);
            match load_or_create_key(&path) {
                Err(KeyFileError::WrongLength { len: got, .. }) => assert_eq!(got, len as u64),
                Err(other) => panic!("{len}: {other:?}"),
                Ok(_) => panic!("{len}: accepted"),
            }
            assert_eq!(
                fs::read(&path).unwrap(),
                vec![7u8; len],
                "file left unchanged"
            );
        }
    }

    #[test]
    fn group_readable_key_is_refused() {
        let dir = TempDir::new().unwrap();
        for open in [0o640, 0o604, 0o602, 0o660, 0o644] {
            let path = write_key(&dir, &format!("k{open:o}"), 32, open);
            match load_or_create_key(&path) {
                Err(KeyFileError::TooOpen { mode, .. }) => assert_eq!(mode, open),
                Err(other) => panic!("{open:o}: {other:?}"),
                Ok(_) => panic!("{open:o}: accepted"),
            }
        }
    }

    #[test]
    fn missing_directory_is_an_io_error() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nope").join("key");
        match load_or_create_key(&path) {
            Err(KeyFileError::Io { path: got, .. }) => assert_eq!(got, path),
            Err(other) => panic!("{other:?}"),
            Ok(_) => panic!("accepted"),
        }
    }

    #[test]
    fn round_trip() {
        let dir = TempDir::new().unwrap();
        let k = key(&dir, "k");
        assert_eq!(decrypt(&k, 7, &encrypt(&k, 7, b"token")).unwrap(), b"token");
    }

    #[test]
    fn other_account_fails() {
        let dir = TempDir::new().unwrap();
        let k = key(&dir, "k");
        let stored = encrypt(&k, 7, b"token");
        assert_eq!(decrypt(&k, 8, &stored), Err(SecretError::Undecryptable));
    }

    #[test]
    fn other_key_fails() {
        let dir = TempDir::new().unwrap();
        let stored = encrypt(&key(&dir, "a"), 7, b"token");
        assert_eq!(
            decrypt(&key(&dir, "b"), 7, &stored),
            Err(SecretError::Undecryptable)
        );
    }

    #[test]
    fn nonce_is_random() {
        let dir = TempDir::new().unwrap();
        let k = key(&dir, "k");
        assert_ne!(encrypt(&k, 7, b"token"), encrypt(&k, 7, b"token"));
    }

    #[test]
    fn corrupt_stored_values_fail() {
        let dir = TempDir::new().unwrap();
        let k = key(&dir, "k");
        let good = encrypt(&k, 7, b"token");
        let mut wrong_version = good.clone();
        wrong_version[0] = 2;
        let mut flipped = good.clone();
        *flipped.last_mut().unwrap() ^= 1;
        for bad in [vec![], vec![1u8; 24], vec![1u8; 25], wrong_version, flipped] {
            assert_eq!(decrypt(&k, 7, &bad), Err(SecretError::Undecryptable));
        }
    }
}
