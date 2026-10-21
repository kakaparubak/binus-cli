use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Serialize, Deserialize)]
pub struct CredentialRecord {
    pub account: String,
    pub access_token: String,
    pub refresh_token: String,
    pub role: RoleContext,
}

impl fmt::Debug for CredentialRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CredentialRecord")
            .field("account", &"[REDACTED]")
            .field("secrets", &"[REDACTED]")
            .field("role", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct RoleContext {
    pub academic_career: String,
    pub institution: String,
    pub name: String,
    pub role_id: String,
    pub organization_id: String,
}

impl fmt::Debug for RoleContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RoleContext")
            .field("values", &"[REDACTED]")
            .finish()
    }
}

#[cfg(any(windows, test))]
pub fn valid_credential_record(record: &CredentialRecord) -> bool {
    let valid_token = |value: &str| {
        value.len() >= 16 && value.len() <= 32_768 && value.bytes().all(|b| b.is_ascii_graphic())
    };
    valid_token(&record.access_token)
        && valid_token(&record.refresh_token)
        && [
            record.role.academic_career.as_str(),
            record.role.institution.as_str(),
            record.role.name.as_str(),
            record.role.role_id.as_str(),
            record.role.organization_id.as_str(),
        ]
        .iter()
        .all(|s| !s.is_empty() && s.len() <= 256)
}
