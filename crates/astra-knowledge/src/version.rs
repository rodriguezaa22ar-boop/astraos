/// Version of the persisted knowledge envelope and public claim contract.
///
/// Existing serialized fields must not be renamed or removed without
/// incrementing this value and adding an explicit migration.
pub const KNOWLEDGE_SCHEMA_VERSION: u32 = 1;

pub(crate) const KNOWLEDGE_ID_PREFIX: &str = "k1-";
