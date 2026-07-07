//! Privacy / hardening score. This is a *view* over the tweak catalog: a curated
//! set of privacy- and security-relevant tweaks whose live `is_applied()` state
//! determines how hardened the system is. Hardening reuses the same reversible
//! apply path as the Tweaks panel, so nothing new is privileged here.
//!
//! Two groups: privacy (stop Windows profiling you) and real security hardening
//! (close attack surface). The security items carry a `warn` explaining what
//! each does and what could break, surfaced in the panel.

/// Tweak ids (from [`crate::core::tweaks::catalog`]) that together define a
/// privacy-respecting, hardened Windows. Order is the display order in the panel.
pub fn check_ids() -> &'static [&'static str] {
    &[
        // Privacy: stop Windows profiling and phoning home.
        "disable-advertising-id",
        "disable-tailored-experiences",
        "disable-start-web-search",
        "disable-start-tracking",
        "disable-suggestions",
        "disable-telemetry",
        "disable-copilot",
        "disable-consumer-features",
        "disable-cortana",
        "svc-diagtrack",
        "svc-dmwappush",
        // Security hardening: close real attack surface (each has a warning).
        "harden-smb1",
        "harden-autorun",
        "harden-smartscreen",
        "harden-llmnr",
        "harden-wdigest",
        "harden-pua",
        "harden-wsh",
        "harden-nolmhash",
        "harden-no-rdp",
        "harden-no-remote-assist",
        "disable-wpbt",
        "block-razer-autoinstall",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tweaks;

    #[test]
    fn every_check_id_resolves_to_a_tweak() {
        let cat = tweaks::catalog();
        for id in check_ids() {
            assert!(
                cat.iter().any(|t| t.id == *id),
                "privacy check `{id}` has no matching tweak"
            );
        }
    }

    #[test]
    fn checks_are_privacy_relevant_and_nonempty() {
        assert!(check_ids().len() >= 8);
    }
}
