//! A small curated catalog of apps installable via `winget`.
//!
//! Installing shells out to `winget install --id <id> -e`. Uninstall/rollback
//! is intentionally out of scope here — installs are not part of the reversible
//! action model.

pub struct App {
    pub name: &'static str,
    pub publisher: &'static str,
    /// winget package id (`--id`, exact match).
    pub id: &'static str,
    pub category: &'static str,
}

pub fn catalog() -> Vec<App> {
    vec![
        App { name: "Visual Studio Code", publisher: "Microsoft", id: "Microsoft.VisualStudioCode", category: "DEV" },
        App { name: "Git", publisher: "Git", id: "Git.Git", category: "DEV" },
        App { name: "PowerToys", publisher: "Microsoft", id: "Microsoft.PowerToys", category: "DEV" },
        App { name: "7-Zip", publisher: "Igor Pavlov", id: "7zip.7zip", category: "UTILITY" },
        App { name: "Notepad++", publisher: "Notepad++ Team", id: "Notepad++.Notepad++", category: "DEV" },
        App { name: "Everything", publisher: "voidtools", id: "voidtools.Everything", category: "UTILITY" },
        App { name: "Firefox", publisher: "Mozilla", id: "Mozilla.Firefox", category: "WEB" },
        App { name: "VLC", publisher: "VideoLAN", id: "VideoLAN.VLC", category: "MEDIA" },
        App { name: "OBS Studio", publisher: "OBS Project", id: "OBSProject.OBSStudio", category: "MEDIA" },
        App { name: "Steam", publisher: "Valve", id: "Valve.Steam", category: "GAMING" },
        App { name: "Discord", publisher: "Discord", id: "Discord.Discord", category: "GAMING" },
    ]
}

/// The full `winget` argument vector for installing an app id.
pub fn install_args(id: &str) -> Vec<String> {
    vec![
        "install".into(),
        "--id".into(),
        id.into(),
        "-e".into(),
        "--accept-source-agreements".into(),
        "--accept-package-agreements".into(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_ids_are_well_formed() {
        for a in catalog() {
            assert!(a.id.contains('.'), "winget id should be Publisher.Package: {}", a.id);
            assert!(!a.name.is_empty());
        }
    }

    #[test]
    fn install_args_shape() {
        let args = install_args("Foo.Bar");
        assert_eq!(args[0], "install");
        assert!(args.contains(&"Foo.Bar".to_string()));
        assert!(args.contains(&"-e".to_string()));
    }
}
