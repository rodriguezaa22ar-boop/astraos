use serde::{de::Error as DeError, Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::{fmt, str::FromStr};

const SHA256_PREFIX: &str = "sha256:";
const SHA256_HEX_LENGTH: usize = 64;

/// A validated, stable SHA-256 fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fingerprint(String);

impl Fingerprint {
    pub fn parse(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        let hex = value
            .strip_prefix(SHA256_PREFIX)
            .ok_or_else(|| "fingerprint must start with sha256:".to_string())?;
        if hex.len() != SHA256_HEX_LENGTH || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("fingerprint must contain exactly 64 hexadecimal characters".to_string());
        }
        if hex.bytes().any(|byte| byte.is_ascii_uppercase()) {
            return Err("fingerprint hexadecimal characters must be lowercase".to_string());
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn from_digest(digest: [u8; 32]) -> Self {
        let mut value = String::with_capacity(SHA256_PREFIX.len() + SHA256_HEX_LENGTH);
        value.push_str(SHA256_PREFIX);
        for byte in digest {
            value.push(hex_digit(byte >> 4));
            value.push(hex_digit(byte & 0x0f));
        }
        Self(value)
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for Fingerprint {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl Serialize for Fingerprint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Fingerprint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(D::Error::custom)
    }
}

pub(crate) fn hash_fields(domain: &str, fields: &[(&str, &[u8])]) -> Fingerprint {
    let mut hasher = Sha256::new();
    write_field(&mut hasher, "domain", domain.as_bytes());
    for (name, value) in fields {
        write_field(&mut hasher, "field", name.as_bytes());
        write_field(&mut hasher, name, value);
    }
    Fingerprint::from_digest(hasher.finalize().into())
}

fn write_field(hasher: &mut Sha256, label: &str, value: &[u8]) {
    hasher.update((label.len() as u64).to_be_bytes());
    hasher.update(label.as_bytes());
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn hex_digit(value: u8) -> char {
    if value < 10 {
        (b'0' + value) as char
    } else if value < 16 {
        (b'a' + value - 10) as char
    } else {
        '?'
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprints_use_validated_lowercase_sha256_format() {
        let fingerprint = hash_fields("test-v1", &[("value", b"one")]);
        assert!(fingerprint.as_str().starts_with("sha256:"));
        assert_eq!(
            fingerprint.as_str().len(),
            SHA256_PREFIX.len() + SHA256_HEX_LENGTH
        );
        assert_eq!(
            Fingerprint::parse(fingerprint.as_str()).expect("valid fingerprint"),
            fingerprint
        );
        assert!(Fingerprint::parse("sha256:ABC").is_err());
        assert!(Fingerprint::parse("md5:00000000000000000000000000000000").is_err());
    }

    #[test]
    fn length_prefixing_distinguishes_ambiguous_field_boundaries() {
        let first = hash_fields("test-v1", &[("a", b"bc"), ("d", b"e")]);
        let second = hash_fields("test-v1", &[("a", b"b"), ("cd", b"e")]);
        assert_ne!(first, second);
    }

    #[test]
    fn serialization_round_trip_preserves_the_contract() {
        let fingerprint = hash_fields("test-v1", &[("value", b"stable")]);
        let json = serde_json::to_string(&fingerprint).expect("fingerprint JSON");
        let restored: Fingerprint = serde_json::from_str(&json).expect("fingerprint round trip");
        assert_eq!(restored, fingerprint);
    }
}
