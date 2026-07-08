//! The built-in system cleaners: the same temporary-files / Recycle Bin /
//! thumbnail / update-cache targets the Cleanup panel has always shown, now
//! expressed as [`Cleaner`]s so they run through the shared engine. Each is a
//! single-option cleaner, which keeps the panel a flat one-row-per-target list.

use super::{CleanAction, Cleaner, CleanerOption, Group, Source};

/// Build a single-option system cleaner.
fn one(id: &str, name: &str, desc: &str, elevated: bool, actions: Vec<CleanAction>) -> Cleaner {
    Cleaner {
        id: id.into(),
        name: name.into(),
        group: Group::System,
        source: Source::Builtin,
        running_procs: Vec::new(),
        options: vec![CleanerOption {
            id: id.into(),
            label: name.into(),
            desc: desc.into(),
            default_on: true,
            warning: None,
            elevated,
            guard_running: false,
            actions,
        }],
    }
}

/// The built-in Windows system cleaners, in display order.
pub fn system_cleaners() -> Vec<Cleaner> {
    vec![
        one(
            "temp",
            "Temporary files",
            "Your user %TEMP% folder.",
            false,
            vec![CleanAction::EmptyDir {
                root: "%TEMP%".into(),
            }],
        ),
        one(
            "recycle",
            "Recycle Bin",
            "Deleted files across all drives.",
            false,
            vec![CleanAction::RecycleBin],
        ),
        one(
            "thumbs",
            "Thumbnail cache",
            "Explorer thumbnail & icon caches.",
            false,
            vec![CleanAction::EmptyDir {
                root: "%LOCALAPPDATA%\\Microsoft\\Windows\\Explorer".into(),
            }],
        ),
        one(
            "syscache",
            "System & update cache",
            "C:\\Windows\\Temp and the Windows Update download cache.",
            true,
            vec![
                CleanAction::EmptyDir {
                    root: "%SystemRoot%\\Temp".into(),
                },
                CleanAction::EmptyDir {
                    root: "%SystemRoot%\\SoftwareDistribution\\Download".into(),
                },
            ],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_matches_the_original_targets() {
        let c = system_cleaners();
        assert_eq!(c.len(), 4);
        let ids: Vec<&str> = c.iter().map(|x| x.id.as_str()).collect();
        assert_eq!(ids, ["temp", "recycle", "thumbs", "syscache"]);
        // syscache is the only elevated target.
        assert!(c.iter().filter(|x| x.options[0].elevated).count() == 1);
        assert!(c.iter().find(|x| x.id == "syscache").unwrap().options[0].elevated);
    }

    #[test]
    fn every_option_is_default_on_and_single() {
        for c in system_cleaners() {
            assert_eq!(c.options.len(), 1, "system cleaners stay single-option");
            assert!(c.options[0].default_on);
        }
    }
}
