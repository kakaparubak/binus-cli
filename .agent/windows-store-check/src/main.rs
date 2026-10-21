#[path = "../../../spikes/001-auth/src/credential_record.rs"]
mod credential_record;
use credential_record::{CredentialRecord, RoleContext};
pub(crate) fn valid_credential_record(r: &CredentialRecord) -> bool {
    credential_record::valid_credential_record(r)
}
#[path = "../../../spikes/001-auth/src/windows_lock.rs"]
mod windows_lock;
#[path = "../../../spikes/001-auth/src/windows_store.rs"]
mod windows_store;
fn main() {
    let _guard = windows_lock::acquire();
    let p = std::path::Path::new("credentials.json");
    let _ = windows_store::store_record(
        p,
        &CredentialRecord {
            account: String::new(),
            access_token: String::new(),
            refresh_token: String::new(),
            role: RoleContext {
                academic_career: String::new(),
                institution: String::new(),
                name: String::new(),
                role_id: String::new(),
                organization_id: String::new(),
            },
        },
    );
    let _ = windows_store::load_record(p);
    let _ = windows_store::remove_record_file(p);
    let _ = windows_store::remove_spike_artifacts(p);
    let _ = windows_store::promote_verified_candidate(
        p,
        std::path::Path::new("credentials.captured.json"),
        &CredentialRecord {
            account: String::new(),
            access_token: String::new(),
            refresh_token: String::new(),
            role: RoleContext {
                academic_career: String::new(),
                institution: String::new(),
                name: String::new(),
                role_id: String::new(),
                organization_id: String::new(),
            },
        },
    );
    let _ = windows_store::load_preferred_record(p);
}
