//! Cross-session mutex used to serialize future Windows credential-store access.
//! This module intentionally does not implement storage.

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AcePolicy {
    pub allowed: bool,
    pub explicit: bool,
    pub mask: u32,
    pub sid_matches: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DescriptorPolicy {
    pub owner_matches: bool,
    pub dacl_present: bool,
    pub dacl_protected: bool,
    pub null_dacl: bool,
    pub aces: Vec<AcePolicy>,
}

pub(crate) fn accepts_descriptor(d: &DescriptorPolicy, all_access: u32) -> bool {
    d.owner_matches
        && d.dacl_present
        && d.dacl_protected
        && !d.null_dacl
        && d.aces.len() == 1
        && d.aces[0].allowed
        && d.aces[0].explicit
        && d.aces[0].mask == all_access
        && d.aces[0].sid_matches
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WaitDecision {
    Acquired,
    Abandoned,
    TimedOut,
    Failed,
}

pub(crate) fn mutex_name_for_sid(canonical_sid: &str) -> String {
    format!("Global\\BinusAuthSpike.Lock.v1.{canonical_sid}")
}

pub(crate) fn wait_decision(result: u32) -> WaitDecision {
    match result {
        0 => WaitDecision::Acquired,
        0x80 => WaitDecision::Abandoned,
        0x102 => WaitDecision::TimedOut,
        _ => WaitDecision::Failed,
    }
}

#[cfg(test)]
mod naming_tests {
    use super::mutex_name_for_sid;

    #[test]
    fn mutex_names_are_sid_scoped_and_versioned() {
        let first = mutex_name_for_sid("S-1-5-21-100-200-300-1001");
        let same = mutex_name_for_sid("S-1-5-21-100-200-300-1001");
        let other = mutex_name_for_sid("S-1-5-21-100-200-300-1002");
        assert_eq!(first, same);
        assert_ne!(first, other);
        assert!(first.starts_with("Global\\BinusAuthSpike.Lock.v1."));
    }
}

#[cfg(windows)]
#[path = "windows_lock/native.rs"]
pub(crate) mod native;
#[cfg(windows)]
pub(crate) use native::acquire;

#[cfg(test)]
mod tests {
    use super::*;
    fn valid() -> DescriptorPolicy {
        DescriptorPolicy {
            owner_matches: true,
            dacl_present: true,
            dacl_protected: true,
            null_dacl: false,
            aces: vec![AcePolicy {
                allowed: true,
                explicit: true,
                mask: 0x1f0001,
                sid_matches: true,
            }],
        }
    }
    #[test]
    fn valid_descriptor_is_accepted() {
        assert!(accepts_descriptor(&valid(), 0x1f0001));
    }
    #[test]
    fn invalid_descriptors_are_rejected() {
        let mut d = valid();
        d.owner_matches = false;
        assert!(!accepts_descriptor(&d, 0x1f0001));
        let mut d = valid();
        d.dacl_present = false;
        assert!(!accepts_descriptor(&d, 0x1f0001));
        let mut d = valid();
        d.dacl_protected = false;
        assert!(!accepts_descriptor(&d, 0x1f0001));
        let mut d = valid();
        d.null_dacl = true;
        assert!(!accepts_descriptor(&d, 0x1f0001));
        let mut d = valid();
        d.aces.push(d.aces[0].clone());
        assert!(!accepts_descriptor(&d, 0x1f0001));
        let mut d = valid();
        d.aces[0].allowed = false;
        assert!(!accepts_descriptor(&d, 0x1f0001));
        let mut d = valid();
        d.aces[0].explicit = false;
        assert!(!accepts_descriptor(&d, 0x1f0001));
        let mut d = valid();
        d.aces[0].sid_matches = false;
        assert!(!accepts_descriptor(&d, 0x1f0001));
        let mut d = valid();
        d.aces[0].mask = 1;
        assert!(!accepts_descriptor(&d, 0x1f0001));
    }
    #[test]
    fn wait_ownership_policy_is_explicit() {
        assert_eq!(wait_decision(0), WaitDecision::Acquired);
        assert_eq!(wait_decision(0x80), WaitDecision::Abandoned);
        assert_eq!(wait_decision(0x102), WaitDecision::TimedOut);
        assert_eq!(wait_decision(u32::MAX), WaitDecision::Failed);
    }
}
