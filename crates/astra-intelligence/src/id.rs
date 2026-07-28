use sha2::{Digest, Sha256};
use std::fmt;

fn derive(prefix: &str, domain: &str, fields: &[&str]) -> String {
    let mut hasher = Sha256::new();
    write_field(&mut hasher, "domain", domain.as_bytes());
    for field in fields {
        write_field(&mut hasher, "field", field.as_bytes());
    }
    let digest = hasher.finalize();
    format!("{prefix}{digest:x}")
}

fn write_field(hasher: &mut Sha256, label: &str, value: &[u8]) {
    hasher.update((label.len() as u64).to_be_bytes());
    hasher.update(label.as_bytes());
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

macro_rules! stable_id {
    ($name:ident, $prefix:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub(crate) fn derive(domain: &str, fields: &[&str]) -> Self {
                Self(derive($prefix, domain, fields))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

stable_id!(EntityId, "pi-entity-v1-");
stable_id!(RelationshipId, "pi-relationship-v1-");
stable_id!(InsightId, "pi-insight-v1-");
stable_id!(RiskId, "pi-risk-v1-");
stable_id!(LimitationId, "pi-limitation-v1-");

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(transparent)]
pub struct RuleId(String);

impl RuleId {
    pub const PI_001: &'static str = "PI-001";
    pub const PI_002: &'static str = "PI-002";
    pub const PI_003: &'static str = "PI-003";
    pub const PI_004: &'static str = "PI-004";
    pub const PI_005: &'static str = "PI-005";
    pub const PI_006: &'static str = "PI-006";

    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RuleId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}
