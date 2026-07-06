//! Quick actions, the Linux analog of the Windows Quick Actions panel: a handful
//! of one-shot maintenance tasks. Each yields an argv plus whether it needs a
//! privilege prompt, so the UI can wrap privileged ones in `pkexec`.

pub struct Action {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub privileged: bool,
}

pub fn catalog() -> &'static [Action] {
    &[
        Action {
            id: "flush-dns",
            name: "Flush DNS cache",
            desc: "Clear systemd-resolved's DNS cache.",
            privileged: false,
        },
        Action {
            id: "clear-cache",
            name: "Clear user cache",
            desc: "Empty ~/.cache.",
            privileged: false,
        },
        Action {
            id: "empty-trash",
            name: "Empty Trash",
            desc: "Delete everything in the desktop Trash.",
            privileged: false,
        },
        Action {
            id: "drop-caches",
            name: "Drop memory caches",
            desc: "sync + drop pagecache/dentries/inodes (frees cached RAM).",
            privileged: true,
        },
        Action {
            id: "autoremove",
            name: "Remove orphaned packages",
            desc: "Purge packages no longer needed by anything (autoremove).",
            privileged: true,
        },
        Action {
            id: "trim-ssd",
            name: "Trim SSDs now",
            desc: "Run fstrim on all mounted filesystems.",
            privileged: true,
        },
        Action {
            id: "enable-flathub",
            name: "Enable Flathub",
            desc: "Add the Flathub remote for Flatpak (per user).",
            privileged: false,
        },
    ]
}

/// Orphaned-package removal for the detected package manager.
fn autoremove_argv() -> Vec<String> {
    match super::pkg::primary() {
        Some(super::pkg::Manager::Apt) => vec!["apt-get".into(), "autoremove".into(), "-y".into()],
        Some(super::pkg::Manager::Dnf) => vec!["dnf".into(), "autoremove".into(), "-y".into()],
        Some(super::pkg::Manager::Pacman) => vec![
            "sh".into(),
            "-c".into(),
            "pacman -Qtdq | pacman -Rns --noconfirm -".into(),
        ],
        Some(super::pkg::Manager::Zypper) => {
            vec![
                "zypper".into(),
                "--non-interactive".into(),
                "packages".into(),
                "--orphaned".into(),
            ]
        }
        _ => vec!["true".into()],
    }
}

/// The command line for an action id, or None if unknown.
pub fn run_argv(id: &str) -> Option<Vec<String>> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    let v: Vec<String> = match id {
        "flush-dns" => vec!["resolvectl".into(), "flush-caches".into()],
        "clear-cache" => vec!["sh".into(), "-c".into(), format!("rm -rf {home}/.cache/*")],
        "empty-trash" => vec![
            "sh".into(),
            "-c".into(),
            format!("rm -rf {home}/.local/share/Trash/files/* {home}/.local/share/Trash/info/*"),
        ],
        "drop-caches" => vec![
            "sh".into(),
            "-c".into(),
            "sync && echo 3 > /proc/sys/vm/drop_caches".into(),
        ],
        "autoremove" => autoremove_argv(),
        "trim-ssd" => vec!["fstrim".into(), "-av".into()],
        "enable-flathub" => vec![
            "flatpak".into(),
            "remote-add".into(),
            "--if-not-exists".into(),
            "--user".into(),
            "flathub".into(),
            "https://flathub.org/repo/flathub.flatpakrepo".into(),
        ],
        _ => return None,
    };
    Some(v)
}

/// Whether an action needs a privilege prompt.
pub fn privileged(id: &str) -> bool {
    catalog()
        .iter()
        .find(|a| a.id == id)
        .map(|a| a.privileged)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flush_dns_is_resolvectl() {
        assert_eq!(
            run_argv("flush-dns").unwrap(),
            vec!["resolvectl", "flush-caches"]
        );
        assert!(!privileged("flush-dns"));
    }

    #[test]
    fn drop_caches_is_privileged() {
        assert!(privileged("drop-caches"));
        assert!(run_argv("drop-caches").is_some());
    }

    #[test]
    fn unknown_action_is_none() {
        assert!(run_argv("nope").is_none());
    }
}
