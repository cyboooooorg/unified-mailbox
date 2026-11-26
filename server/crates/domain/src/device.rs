//! Devices: the clients paired with the server, their names, and the
//! one-time pairing codes that enrol them.

use std::fmt;

use chrono::{DateTime, TimeDelta, Utc};
use thiserror::Error;

/// How long a pairing code stays valid after its creation.
pub const PAIRING_CODE_TTL: TimeDelta = TimeDelta::minutes(10);

/// A device's `last_seen_at` is written only when unset or at least this
/// old, so authenticated requests do not each write to the store.
pub const LAST_SEEN_RESOLUTION: TimeDelta = TimeDelta::minutes(1);

/// Crockford base32: no `I`, `L`, `O` or `U`.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// The identifier of a paired device, assigned by the store.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct DeviceId(i64);

impl DeviceId {
    pub fn new(id: i64) -> DeviceId {
        DeviceId(id)
    }

    pub fn get(self) -> i64 {
        self.0
    }
}

/// The name of a device: trimmed, 1 to 64 characters, no control
/// character. Duplicates are allowed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceName(String);

impl DeviceName {
    pub const MAX_CHARS: usize = 64;

    pub fn new(raw: &str) -> Result<DeviceName, DeviceNameError> {
        let name = raw.trim();
        match name.chars().count() {
            0 => return Err(DeviceNameError::Empty),
            n if n > Self::MAX_CHARS => {
                return Err(DeviceNameError::TooLong {
                    max: Self::MAX_CHARS,
                });
            }
            _ => {}
        }
        if name.chars().any(char::is_control) {
            return Err(DeviceNameError::ControlCharacter);
        }
        Ok(DeviceName(name.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Errors of [`DeviceName`]. Messages are codes: the client translates them.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum DeviceNameError {
    #[error("device_name_empty")]
    Empty,
    #[error("device_name_too_long")]
    TooLong { max: usize },
    #[error("device_name_control_character")]
    ControlCharacter,
}

/// A paired device. A revoked device keeps its row but can no longer
/// authenticate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub id: DeviceId,
    pub name: DeviceName,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

/// A one-time pairing code: 8 Crockford base32 characters (40 bits), shown
/// as `XXXX-XXXX`.
///
/// It is a secret until used: `Debug` never prints it.
#[derive(Clone, PartialEq, Eq)]
pub struct PairingCode(String);

impl PairingCode {
    /// Encodes 40 bits, most significant first, 5 bits per character.
    pub fn from_bytes(bytes: [u8; 5]) -> PairingCode {
        let bits = bytes.iter().fold(0u64, |acc, b| (acc << 8) | u64::from(*b));
        let code = (0..8)
            .map(|i| ALPHABET[((bits >> (35 - 5 * i)) & 31) as usize] as char)
            .collect();
        PairingCode(code)
    }

    /// Reads what a user typed: whitespace and `-` are dropped, case is
    /// ignored, `O` reads as `0` and `I` and `L` as `1`. Anything else
    /// outside the alphabet, or a length other than 8, is rejected.
    pub fn parse(raw: &str) -> Result<PairingCode, InvalidPairingCode> {
        let code: String = raw
            .chars()
            .filter(|c| !c.is_ascii_whitespace() && *c != '-')
            .map(|c| match c.to_ascii_uppercase() {
                'O' => '0',
                'I' | 'L' => '1',
                other => other,
            })
            .collect();
        let valid = code.len() == 8 && code.bytes().all(|b| ALPHABET.contains(&b));
        if valid {
            Ok(PairingCode(code))
        } else {
            Err(InvalidPairingCode)
        }
    }

    /// The 8 normalized characters, without the hyphen.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PairingCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", &self.0[..4], &self.0[4..])
    }
}

impl fmt::Debug for PairingCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PairingCode(..)")
    }
}

/// The text is not a pairing code.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[error("invalid_pairing_code")]
pub struct InvalidPairingCode;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_trimmed() {
        assert_eq!(DeviceName::new("  Phone ").unwrap().as_str(), "Phone");
    }

    #[test]
    fn name_empty_after_trim() {
        assert_eq!(DeviceName::new(" \t "), Err(DeviceNameError::Empty));
    }

    #[test]
    fn name_counts_chars_not_bytes() {
        assert!(DeviceName::new(&"é".repeat(64)).is_ok());
        assert_eq!(
            DeviceName::new(&"é".repeat(65)),
            Err(DeviceNameError::TooLong { max: 64 })
        );
    }

    #[test]
    fn name_rejects_control_character() {
        assert_eq!(
            DeviceName::new("Pho\u{7}ne"),
            Err(DeviceNameError::ControlCharacter)
        );
    }

    #[test]
    fn duplicate_names_are_values() {
        assert_eq!(DeviceName::new("Phone"), DeviceName::new("Phone"));
    }

    #[test]
    fn code_from_bytes_uses_crockford() {
        assert_eq!(PairingCode::from_bytes([0; 5]).as_str(), "00000000");
        assert_eq!(PairingCode::from_bytes([0xff; 5]).as_str(), "ZZZZZZZZ");
        // 0x0123456789 = 00000 00100 10001 10100 01010 11001 11100 01001
        assert_eq!(
            PairingCode::from_bytes([0x01, 0x23, 0x45, 0x67, 0x89]).as_str(),
            "04HMASW9"
        );
    }

    #[test]
    fn code_displays_with_hyphen() {
        assert_eq!(
            PairingCode::from_bytes([0x01, 0x23, 0x45, 0x67, 0x89]).to_string(),
            "04HM-ASW9"
        );
    }

    #[test]
    fn code_parse_normalizes() {
        for input in ["04HM-ASW9", "04hmasw9", " 04hm asw9 ", "O4HM-ASW9"] {
            assert_eq!(
                PairingCode::parse(input).unwrap().as_str(),
                "04HMASW9",
                "{input}"
            );
        }
        assert_eq!(
            PairingCode::parse("1ILl-0000").unwrap().as_str(),
            "11110000"
        );
    }

    #[test]
    fn code_parse_rejects() {
        for input in ["", "04HM-ASW", "04HM-ASW90", "04HM-ASWU", "04HM_ASW9"] {
            assert_eq!(
                PairingCode::parse(input),
                Err(InvalidPairingCode),
                "{input}"
            );
        }
    }

    #[test]
    fn code_debug_hides_value() {
        assert_eq!(
            format!("{:?}", PairingCode::from_bytes([0; 5])),
            "PairingCode(..)"
        );
    }
}
