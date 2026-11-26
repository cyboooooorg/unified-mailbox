//! Bearer tokens and the hashes under which tokens and pairing codes are
//! stored.

use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use domain::PairingCode;
use ports::SecretGenerator;
use sha2::{Digest, Sha256};

const PREFIX: &str = "um_";
/// Random bytes in a token: 256 bits.
const TOKEN_BYTES: usize = 32;
/// Characters of 32 bytes in base64url without padding.
const ENCODED_LEN: usize = 43;

/// A freshly issued bearer token: `um_` then 32 random bytes in base64url
/// without padding (46 characters). The prefix lets secret scanners
/// recognize it.
///
/// It is shown once to the device that paired; `Debug` never prints it.
pub struct DeviceToken(String);

impl DeviceToken {
    pub(crate) fn generate(secrets: &dyn SecretGenerator) -> DeviceToken {
        let mut bytes = [0u8; TOKEN_BYTES];
        secrets.fill(&mut bytes);
        DeviceToken(format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(bytes)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for DeviceToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeviceToken(..)")
    }
}

/// The stored form of a token: lowercase hex SHA-256 of the whole string,
/// prefix included. The token's entropy makes a slow hash useless.
pub fn hash_token(token: &str) -> String {
    hex_sha256(token.as_bytes())
}

/// The stored form of a pairing code: lowercase hex SHA-256 of its 8
/// normalized characters.
pub fn hash_pairing_code(code: &PairingCode) -> String {
    hex_sha256(code.as_str().as_bytes())
}

/// Whether `input` has the shape of a token (`um_` then 43 characters of
/// `[A-Za-z0-9_-]`), checked before any lookup.
pub fn looks_like_token(input: &str) -> bool {
    input.strip_prefix(PREFIX).is_some_and(|body| {
        body.len() == ENCODED_LEN
            && body
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    })
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
