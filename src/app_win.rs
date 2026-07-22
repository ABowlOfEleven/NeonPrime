// NeonPrime, the Windows desktop UI binary.
//
// This file is `include!`d at the crate root by `src/main.rs` only on Windows,
// so every Windows-only module and dependency below is gated by construction.
mod cputemp;
mod gpu;
mod sensors;
mod telemetry;

use std::cell::{Cell, RefCell};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use slint::{FilterModel, Model, ModelRc, Timer, TimerMode, VecModel};

use neonprime::core::action::{Action, Reversal};
use neonprime::core::ipc::{Request, Response};
use neonprime::core::journal::Journal;
use neonprime::core::session::BrokerSession;
use neonprime::core::{
    asset, bundle, certs, cleaners, config, debloat, devices, disks, dns, engine, eventlog, features,
    firewall, gpo, hidden_command, installs, journal, localusers, microwin, modes, netmon, posture,
    power, printers, privacy, procmon, profiles, quick, repair, services, settings, startup, tweaks,
};

use telemetry::{Sample, Telemetry};

slint::include_modules!();

type SharedJournal = Rc<RefCell<Journal>>;
/// `notify(kind, message)`, kind is "success" | "error" | "info".
type Notify = Rc<dyn Fn(&str, &str)>;

/// Result of an off-thread elevated tweak, marshalled back to the UI thread.
/// Only `Send` data crosses the boundary (no `Rc`).
enum ElevatedMsg {
    Done {
        row_id: i32,
        name: String,
        want: bool,
        results: Vec<(Action, Reversal)>,
    },
    Failed {
        row_id: i32,
        name: String,
        error: String,
    },
}

/// Result of an elevated *revert* (History panel) coming back from the broker.
enum RevertMsg {
    Done { id: u64, label: String },
    Failed { label: String, error: String },
}

/// Background results for the Debloat panel.
enum DebloatMsg {
    Probed(std::collections::HashSet<String>),
    Removed {
        idx: i32,
        ok: bool,
        name: String,
        err: String,
    },
}

/// Background results for the Cleanup panel.
enum CleanMsg {
    Scanned(Vec<u64>),
    Cleaned { idx: i32, size: u64, name: String },
}

// ── Toast notifier ──────────────────────────────────────────────────

fn make_notifier(app: &AppWindow) -> Notify {
    let weak = app.as_weak();
    let generation = Rc::new(Cell::new(0u64));
    Rc::new(move |kind: &str, msg: &str| {
        let Some(app) = weak.upgrade() else { return };
        let id = generation.get().wrapping_add(1);
        generation.set(id);

        let ui = app.global::<Ui>();
        ui.set_toast_kind(kind.into());
        ui.set_toast_message(msg.into());

        // Auto-clear after a few seconds, unless a newer toast superseded us.
        let weak2 = app.as_weak();
        let gen2 = generation.clone();
        Timer::single_shot(Duration::from_secs(4), move || {
            if gen2.get() == id {
                if let Some(app) = weak2.upgrade() {
                    app.global::<Ui>().set_toast_message("".into());
                }
            }
        });
    })
}

// ── Telemetry ───────────────────────────────────────────────────────

fn apply_telemetry(app: &AppWindow, s: &Sample) {
    let sys = app.global::<Sys>();
    sys.set_cpu_ratio(s.cpu_ratio);
    sys.set_cpu_text(s.cpu_text.as_str().into());
    sys.set_cpu_temp_ratio(s.cpu_temp_ratio);
    sys.set_cpu_temp_text(s.cpu_temp_text.as_str().into());
    sys.set_cpu_temp_warn(s.cpu_temp_warn);
    sys.set_gpu_name(s.gpu_name.as_str().into());
    sys.set_ram_ratio(s.ram_ratio);
    sys.set_ram_text(s.ram_text.as_str().into());
    sys.set_gpu_available(s.gpu_available);
    sys.set_gpu_ratio(s.gpu_ratio);
    sys.set_gpu_text(s.gpu_text.as_str().into());
    sys.set_vram_ratio(s.vram_ratio);
    sys.set_vram_text(s.vram_text.as_str().into());
    sys.set_temp_ratio(s.temp_ratio);
    sys.set_temp_text(s.temp_text.as_str().into());
    sys.set_temp_warn(s.temp_warn);
    sys.set_spec_uptime(s.uptime_text.as_str().into());
}

const SPARK_LEN: usize = 60;

/// Push a sample into a capped ring buffer (oldest dropped past `SPARK_LEN`).
fn spark_push(buf: &mut std::collections::VecDeque<f32>, v: f32) {
    if buf.len() >= SPARK_LEN {
        buf.pop_front();
    }
    buf.push_back(v.clamp(0.0, 1.0));
}

/// Snapshot a history buffer into a Slint float model (newest last).
fn spark_model(buf: &std::collections::VecDeque<f32>) -> ModelRc<f32> {
    ModelRc::new(VecModel::from(buf.iter().copied().collect::<Vec<f32>>()))
}

/// One-time static system specs (OS / CPU / RAM) for the Dashboard strip.
fn apply_specs(app: &AppWindow) {
    let sp = sysinfo::System::new_all();
    let cpu = sp
        .cpus()
        .first()
        .map(|c| c.brand().trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Unknown CPU".into());
    let os = sysinfo::System::long_os_version().unwrap_or_else(|| "Windows".into());
    let ram = format!(
        "{:.0} GiB",
        sp.total_memory() as f64 / (1024.0 * 1024.0 * 1024.0)
    );

    let sys = app.global::<Sys>();
    sys.set_spec_os(os.as_str().into());
    sys.set_spec_cpu(cpu.as_str().into());
    sys.set_spec_ram(ram.as_str().into());
}

// ── Tweaks ──────────────────────────────────────────────────────────

fn make_row(index: usize, t: &tweaks::Tweak) -> TweakRow {
    TweakRow {
        id: index as i32,
        name: t.name.into(),
        desc: t.desc.into(),
        category: t.category.label().into(),
        applied: t.is_applied(),
        elevated: t.needs_elevation(),
    }
}

/// Search/category predicate for a tweak row. `text` is already lowercased.
/// Security-hardening tweaks live in the Privacy/Hardening panel, so they are
/// hidden from the Tweaks list (the "ALL" view excludes SECURITY).
fn tweak_matches(row: &TweakRow, text: &str, cat: &str) -> bool {
    let cat_ok = if cat == "ALL" {
        row.category.as_str() != "SECURITY"
    } else {
        row.category.as_str() == cat
    };
    let text_ok = text.is_empty()
        || row.name.to_lowercase().contains(text)
        || row.desc.to_lowercase().contains(text);
    cat_ok && text_ok
}

/// Re-probe every tweak row from live registry state.
fn refresh_tweaks(model: &VecModel<TweakRow>, catalog: &[tweaks::Tweak]) {
    for (i, t) in catalog.iter().enumerate() {
        model.set_row_data(i, make_row(i, t));
    }
}

/// Sync the active-mode highlight from the marker.
fn refresh_modes(app: &AppWindow, catalog: &[modes::Mode]) {
    let idx = modes::active()
        .and_then(|id| catalog.iter().position(|m| m.id == id))
        .map(|i| i as i32)
        .unwrap_or(-1);
    app.global::<Modes>().set_active(idx);
}

fn run_local(
    actions: &[Action],
    jrnl: &SharedJournal,
    t: &tweaks::Tweak,
    want: bool,
) -> io::Result<()> {
    for a in actions {
        let reversal = engine::apply(a)?;
        jrnl.borrow_mut().record(
            format!("{}: {}", t.name, if want { "on" } else { "off" }),
            a.clone(),
            reversal,
        );
    }
    Ok(())
}

/// Worker-thread body: spawn/reuse the elevated broker (UAC), apply the actions,
/// and report back over the channel. Runs OFF the UI thread so the UAC prompt
/// never freezes the window.
fn elevated_worker(
    broker: Arc<Mutex<Option<BrokerSession>>>,
    tx: mpsc::Sender<ElevatedMsg>,
    actions: Vec<Action>,
    row_id: i32,
    name: String,
    want: bool,
) {
    let mut guard = broker.lock().unwrap();
    if guard.is_none() {
        match BrokerSession::spawn(true) {
            Ok(s) => *guard = Some(s),
            Err(e) => {
                let _ = tx.send(ElevatedMsg::Failed {
                    row_id,
                    name,
                    error: format!("elevation failed: {e}"),
                });
                return;
            }
        }
    }
    let session = guard.as_mut().unwrap();
    let mut results = Vec::new();
    for a in &actions {
        match session.client.call(&Request::Apply {
            label: name.clone(),
            action: a.clone(),
        }) {
            Ok(Response::Applied { reversal }) => results.push((a.clone(), reversal)),
            Ok(Response::Error(e)) => {
                let _ = tx.send(ElevatedMsg::Failed {
                    row_id,
                    name,
                    error: e,
                });
                return;
            }
            Ok(_) => {}
            Err(e) => {
                *guard = None; // drop a dead broker so the next attempt respawns it
                let _ = tx.send(ElevatedMsg::Failed {
                    row_id,
                    name,
                    error: format!("broker link lost: {e}"),
                });
                return;
            }
        }
    }
    let _ = tx.send(ElevatedMsg::Done {
        row_id,
        name,
        want,
        results,
    });
}

/// Wire the Tweaks panel. Returns the result-pump `Timer`, which the caller must
/// keep alive for the lifetime of the app.
fn wire_tweaks(
    app: &AppWindow,
    jrnl: &SharedJournal,
    journal_path: &Path,
    notify: &Notify,
    catalog: &Rc<Vec<tweaks::Tweak>>,
    model: &Rc<VecModel<TweakRow>>,
) -> Timer {
    let broker: Arc<Mutex<Option<BrokerSession>>> = Arc::new(Mutex::new(None));
    let (tx, rx) = mpsc::channel::<ElevatedMsg>();

    // Live search/category filter over the full source model. Toggles still
    // address rows by catalog id, so filtering never desyncs the source.
    let filter_state = Rc::new(RefCell::new((String::new(), "ALL".to_string())));
    let filtered = Rc::new(FilterModel::new(ModelRc::from(model.clone()), {
        let fs = filter_state.clone();
        move |row: &TweakRow| {
            let f = fs.borrow();
            tweak_matches(row, &f.0, &f.1)
        }
    }));
    app.global::<Tweaks>()
        .set_rows(ModelRc::from(filtered.clone()));
    {
        let weak = app.as_weak();
        let fs = filter_state.clone();
        let filtered = filtered.clone();
        app.global::<Tweaks>().on_filter(move || {
            if let Some(app) = weak.upgrade() {
                let t = app.global::<Tweaks>();
                *fs.borrow_mut() = (
                    t.get_filter_text().to_lowercase(),
                    t.get_filter_cat().to_string(),
                );
                filtered.reset();
            }
        });
    }

    // One-click "Essential Tweaks", applies the curated no-elevation set.
    {
        let cat = catalog.clone();
        let model = model.clone();
        let jrnl = jrnl.clone();
        let path = journal_path.to_path_buf();
        let notify = notify.clone();
        app.global::<Tweaks>().on_apply_essential(move || {
            let mut n = 0;
            for id in tweaks::essential_ids() {
                if let Some(t) = cat.iter().find(|t| t.id == *id) {
                    if t.needs_elevation() {
                        continue;
                    }
                    if run_local(&t.on, &jrnl, t, true).is_ok() {
                        n += 1;
                    }
                }
            }
            let _ = jrnl.borrow().save(&path);
            refresh_tweaks(&model, &cat);
            notify("success", &format!("Applied {n} essential tweaks"));
        });
    }

    {
        let cat = catalog.clone();
        let model = model.clone();
        let jrnl = jrnl.clone();
        let path = journal_path.to_path_buf();
        let notify = notify.clone();
        let broker = broker.clone();
        let tx = tx.clone();

        app.global::<Tweaks>().on_toggle(move |id, want| {
            let Some(t) = cat.get(id as usize) else {
                return;
            };

            if t.needs_elevation() {
                // Optimistic UI now; the privileged work happens off-thread so a
                // UAC prompt can't freeze the window. The pump corrects on failure.
                let mut r = make_row(id as usize, t);
                r.applied = want;
                model.set_row_data(id as usize, r);
                notify("info", "Requesting elevation, approve the UAC prompt…");

                let actions: Vec<Action> = if want { t.on.clone() } else { t.off.clone() };
                std::thread::spawn({
                    let broker = broker.clone();
                    let tx = tx.clone();
                    let name = t.name.to_string();
                    move || elevated_worker(broker, tx, actions, id, name, want)
                });
            } else {
                let actions = if want { &t.on } else { &t.off };
                match run_local(actions, &jrnl, t, want) {
                    Ok(()) => notify(
                        "success",
                        &format!("{} {}", t.name, if want { "applied" } else { "reverted" }),
                    ),
                    Err(e) => notify("error", &format!("{}: {}", t.name, e)),
                }
                let _ = jrnl.borrow().save(&path);
                // Re-probe: on failure the row snaps back to reality.
                model.set_row_data(id as usize, make_row(id as usize, t));
            }
        });
    }

    // Result pump (UI thread): drain worker messages and apply them safely.
    let cat = catalog.clone();
    let model = model.clone();
    let jrnl = jrnl.clone();
    let path = journal_path.to_path_buf();
    let notify = notify.clone();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(150), move || {
        while let Ok(msg) = rx.try_recv() {
            match msg {
                ElevatedMsg::Done {
                    row_id,
                    name,
                    want,
                    results,
                } => {
                    {
                        let mut j = jrnl.borrow_mut();
                        for (a, rev) in results {
                            j.record(
                                format!("{}: {}", name, if want { "on" } else { "off" }),
                                a,
                                rev,
                            );
                        }
                    }
                    let _ = jrnl.borrow().save(&path);
                    if let Some(t) = cat.get(row_id as usize) {
                        model.set_row_data(row_id as usize, make_row(row_id as usize, t));
                    }
                    notify(
                        "success",
                        &format!("{} {}", name, if want { "applied" } else { "reverted" }),
                    );
                }
                ElevatedMsg::Failed {
                    row_id,
                    name,
                    error,
                } => {
                    if let Some(t) = cat.get(row_id as usize) {
                        model.set_row_data(row_id as usize, make_row(row_id as usize, t));
                    }
                    notify("error", &format!("{}: {}", name, error));
                }
            }
        }
    });
    timer
}

// ── Modes ───────────────────────────────────────────────────────────

/// Revert the currently-active mode: restore its power scheme and undo every
/// journaled "mode:" action (all HKCU, so reverted locally). Leaves no mode set.
fn deactivate_mode(jrnl: &SharedJournal, path: &Path) {
    if let Some(prev) = modes::take_prev_power() {
        power::set_active(&prev);
    }
    let entries: Vec<journal::Entry> = jrnl
        .borrow()
        .entries
        .iter()
        .filter(|e| e.active && e.label.starts_with("mode:"))
        .cloned()
        .collect();
    for e in entries {
        if engine::revert(&e.reversal).is_ok() {
            jrnl.borrow_mut().mark_reverted(e.id);
        }
    }
    modes::clear_marker();
    let _ = jrnl.borrow().save(path);
}

fn wire_modes(
    app: &AppWindow,
    jrnl: &SharedJournal,
    journal_path: &Path,
    notify: &Notify,
    catalog: &Rc<Vec<modes::Mode>>,
) {
    let weak = app.as_weak();
    let cat = catalog.clone();
    let jrnl = jrnl.clone();
    let path = journal_path.to_path_buf();
    let notify = notify.clone();

    app.global::<Modes>().on_activate(move |idx| {
        let Some(m) = cat.get(idx as usize) else {
            return;
        };

        // Clicking the active mode again turns it off (restores defaults).
        let already = modes::active();
        deactivate_mode(&jrnl, &path);
        if already.as_deref() == Some(m.id) {
            if let Some(app) = weak.upgrade() {
                app.global::<Modes>().set_active(-1);
            }
            notify("info", &format!("{} mode off", m.name));
            return;
        }

        // Apply the new mode's reversible registry actions.
        let mut ok = true;
        for a in &m.actions {
            match engine::apply(a) {
                Ok(reversal) => {
                    jrnl.borrow_mut()
                        .record(format!("mode: {}", m.name), a.clone(), reversal);
                }
                Err(e) => {
                    ok = false;
                    notify("error", &format!("Mode {}: {}", m.name, e));
                }
            }
        }
        // Switch power plan, remembering the current one to restore on exit.
        if let Some(guid) = m.power_guid {
            if let Some(prev) = power::active_guid() {
                modes::save_prev_power(&prev);
            }
            power::set_active(guid);
        }
        modes::set_marker(m.id);
        let _ = jrnl.borrow().save(&path);
        if let Some(app) = weak.upgrade() {
            app.global::<Modes>().set_active(idx);
        }
        if ok {
            notify("success", &format!("{} mode active", m.name));
        }
    });
}

// ── Installs ────────────────────────────────────────────────────────

fn wire_installs(app: &AppWindow, notify: &Notify) -> Timer {
    let catalog = Rc::new(installs::catalog());

    // Display rows: a collapsible header whenever the category changes, then that
    // category's apps. App rows carry their catalog index in `id`; header rows use
    // id = -1. Ordered by (category, name) so apps group under their header.
    let mut order: Vec<usize> = (0..catalog.len()).collect();
    order.sort_by(|&a, &b| {
        let (ca, cb) = (&catalog[a], &catalog[b]);
        ca.category
            .to_lowercase()
            .cmp(&cb.category.to_lowercase())
            .then_with(|| ca.name.to_lowercase().cmp(&cb.name.to_lowercase()))
    });
    let mut display: Vec<AppRow> = Vec::with_capacity(catalog.len() + 16);
    let mut last_cat = String::new();
    for &ci in &order {
        let a = &catalog[ci];
        if a.category != last_cat {
            last_cat.clone_from(&a.category);
            display.push(AppRow {
                id: -1,
                name: Default::default(),
                desc: Default::default(),
                category: a.category.as_str().into(),
                installed: false,
                known: false,
                icon: Default::default(),
                monogram: Default::default(),
                is_header: true,
                collapsed: false,
            });
        }
        display.push(AppRow {
            id: ci as i32,
            name: a.name.as_str().into(),
            desc: a.desc.as_str().into(),
            category: a.category.as_str().into(),
            installed: false,
            known: false,
            icon: Default::default(),
            monogram: monogram_of(&a.name).into(),
            is_header: false,
            collapsed: false,
        });
    }
    let source = Rc::new(VecModel::from(display));

    // catalog id -> source row index, for the scan and icon pumps.
    let mut id_to_row = std::collections::HashMap::<i32, usize>::new();
    for i in 0..source.row_count() {
        if let Some(r) = source.row_data(i) {
            if !r.is_header && r.id >= 0 {
                id_to_row.insert(r.id, i);
            }
        }
    }
    let id_to_row = Rc::new(id_to_row);

    // Search filter over name / description / category, plus category collapse.
    let collapsed = Rc::new(RefCell::new(std::collections::HashSet::<String>::new()));
    let filter_text = Rc::new(RefCell::new(String::new()));
    let filtered = Rc::new(FilterModel::new(ModelRc::from(source.clone()), {
        let ft = filter_text.clone();
        let collapsed = collapsed.clone();
        move |row: &AppRow| {
            let t = ft.borrow();
            let searching = !t.is_empty();
            if row.is_header {
                // Results are flat while searching, so hide category headers.
                return !searching;
            }
            let matches = !searching
                || row.name.to_lowercase().contains(t.as_str())
                || row.desc.to_lowercase().contains(t.as_str())
                || row.category.to_lowercase().contains(t.as_str());
            if !matches {
                return false;
            }
            // Hide apps in a collapsed category (unless searching).
            searching || !collapsed.borrow().contains(row.category.as_str())
        }
    }));
    app.global::<Installer>()
        .set_rows(ModelRc::from(filtered.clone()));
    app.global::<Installer>().set_count(catalog.len() as i32);
    {
        let weak = app.as_weak();
        let ft = filter_text.clone();
        let filtered = filtered.clone();
        app.global::<Installer>().on_filter(move || {
            if let Some(app) = weak.upgrade() {
                *ft.borrow_mut() = app.global::<Installer>().get_filter_text().to_lowercase();
                filtered.reset();
            }
        });
    }
    {
        let collapsed = collapsed.clone();
        let filtered = filtered.clone();
        let source = source.clone();
        app.global::<Installer>().on_toggle_category(move |cat| {
            let cat = cat.to_string();
            {
                let mut c = collapsed.borrow_mut();
                if !c.remove(&cat) {
                    c.insert(cat.clone());
                }
            }
            // Reflect the new state on the header row so its chevron rotates.
            let is_col = collapsed.borrow().contains(&cat);
            for i in 0..source.row_count() {
                if let Some(mut r) = source.row_data(i) {
                    if r.is_header && r.category.as_str() == cat {
                        r.collapsed = is_col;
                        source.set_row_data(i, r);
                        break;
                    }
                }
            }
            filtered.reset();
        });
    }

    let notify2 = notify.clone();
    {
        // Install in a visible, elevated console so the user sees winget's
        // progress and machine-scope packages can elevate (winget spawned hidden
        // and unelevated from the GUI would silently do nothing).
        let cat = catalog.clone();
        let notify = notify.clone();
        app.global::<Installer>().on_install(move |id| {
            let Some(a) = cat.get(id as usize) else { return };
            if a.repo.is_empty() {
                match launch_elevated_ps(&installs::install_cmd(&a.id), true) {
                    Ok(()) => notify("info", &format!("Installing {} (approve UAC)…", a.name)),
                    Err(e) => notify("error", &format!("Couldn't start winget: {e}")),
                }
            } else {
                // GitHub-release app: write the download + install script to a
                // temp .ps1 and run it elevated (a file sidesteps -Command
                // quoting for the multi-statement script).
                let mut p = std::env::temp_dir();
                p.push(format!("neonprime-install-{}.ps1", a.repo.replace('/', "-")));
                match std::fs::write(&p, installs::github_install_script(&a.repo))
                    .and_then(|()| launch_elevated_file(&p, false))
                {
                    Ok(()) => notify(
                        "info",
                        &format!("Installing {} from GitHub (approve UAC)…", a.name),
                    ),
                    Err(e) => notify("error", &format!("Couldn't start installer: {e}")),
                }
            }
        });
    }
    {
        let cat = catalog.clone();
        let notify = notify.clone();
        app.global::<Installer>().on_remove(move |id| {
            let Some(a) = cat.get(id as usize) else { return };
            let cmd = if a.repo.is_empty() {
                installs::uninstall_cmd(&a.id)
            } else {
                installs::uninstall_named_cmd(&a.name)
            };
            match launch_elevated_ps(&cmd, true) {
                Ok(()) => notify("info", &format!("Removing {} (approve UAC)…", a.name)),
                Err(e) => notify("error", &format!("Couldn't start winget: {e}")),
            }
        });
    }

    {
        let notify = notify2.clone();
        app.global::<Installer>().on_update_all(move || {
            match launch_console("winget upgrade --all --include-unknown") {
                Ok(()) => notify("info", "Updating all apps, see the console window."),
                Err(e) => notify("error", &format!("winget: {e}")),
            }
        });
    }

    // App icons: a background thread fetches each favicon (DuckDuckGo, cached to
    // disk via curl), sending (catalog-id, path) back; the pump timer below decodes
    // and sets them on the rows. Gated by the "show app icons" setting.
    let (icon_tx, icon_rx) = mpsc::channel::<(i32, std::path::PathBuf)>();
    // (catalog-id, link) for apps that have a homepage. Arc so the fetch thread
    // can own a Send-able snapshot (the catalog itself is Rc, not Send).
    let icon_links: std::sync::Arc<Vec<(i32, String)>> = std::sync::Arc::new(
        catalog
            .iter()
            .enumerate()
            .filter(|(_, a)| !a.link.is_empty())
            .map(|(i, a)| (i as i32, a.link.clone()))
            .collect(),
    );
    let spawn_icon_fetch: Rc<dyn Fn()> = {
        let icon_tx = icon_tx.clone();
        let icon_links = icon_links.clone();
        Rc::new(move || {
            let icon_tx = icon_tx.clone();
            let icon_links = icon_links.clone();
            std::thread::spawn(move || {
                // Phase 1: already-cached icons, instantly (no network), so the
                // list fills immediately on a warm cache.
                for (id, link) in icon_links.iter() {
                    if let Some(path) = installs::cached_icon(link) {
                        let _ = icon_tx.send((*id, path));
                    }
                }
                // Phase 2: fetch the ones still missing. A slow/uncached app can't
                // hold up the cached ones anymore.
                for (id, link) in icon_links.iter() {
                    if installs::cached_icon(link).is_none() {
                        if let Some(path) = installs::ensure_icon(link) {
                            let _ = icon_tx.send((*id, path));
                        }
                    }
                }
            });
        })
    };
    let icons_on = settings::Settings::load().show_app_icons;
    app.global::<Installer>().set_show_icons(icons_on);
    if icons_on {
        spawn_icon_fetch();
    }
    {
        let weak = app.as_weak();
        let spawn = spawn_icon_fetch.clone();
        let source = source.clone();
        app.global::<Installer>().on_toggle_icons(move || {
            let Some(app) = weak.upgrade() else { return };
            let mut s = settings::Settings::load();
            s.show_app_icons = !s.show_app_icons;
            let now_on = s.show_app_icons;
            s.save();
            app.global::<Installer>().set_show_icons(now_on);
            if now_on {
                spawn();
            } else {
                // Clear loaded icons so a later re-enable starts clean.
                for i in 0..source.row_count() {
                    if let Some(mut r) = source.row_data(i) {
                        if !r.is_header {
                            r.icon = slint::Image::default();
                            source.set_row_data(i, r);
                        }
                    }
                }
            }
        });
    }

    // Scan installed apps off-thread (`winget export` is slow), then flag each row
    // installed / available. An empty scan result means winget failed or is
    // missing, so we leave those rows "unknown" rather than falsely marking
    // everything uninstalled.
    app.global::<Installer>().set_scanning(true);
    let (tx, rx) = mpsc::channel::<installs::Installed>();
    let scan: Rc<dyn Fn()> = {
        let tx = tx.clone();
        Rc::new(move || {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(installs::scan_installed());
            });
        })
    };
    scan();

    {
        let weak = app.as_weak();
        let scan = scan.clone();
        let source = source.clone();
        app.global::<Installer>().on_recheck(move || {
            for i in 0..source.row_count() {
                if let Some(mut r) = source.row_data(i) {
                    r.known = false;
                    source.set_row_data(i, r);
                }
            }
            if let Some(app) = weak.upgrade() {
                app.global::<Installer>().set_scanning(true);
            }
            scan();
        });
    }

    // Pump scan results and fetched icons onto the rows.
    let weak = app.as_weak();
    let catalog2 = catalog.clone();
    let source2 = source.clone();
    let id_to_row2 = id_to_row.clone();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(200), move || {
        while let Ok(scan) = rx.try_recv() {
            // If winget produced nothing (missing/errored), leave winget apps
            // "unknown" rather than falsely marking them uninstalled; author apps
            // resolve from Add/Remove Programs regardless.
            let winget_ok = !scan.ids.is_empty();
            for (ci, a) in catalog2.iter().enumerate() {
                let Some(&ri) = id_to_row2.get(&(ci as i32)) else {
                    continue;
                };
                if let Some(mut r) = source2.row_data(ri) {
                    if !a.id.is_empty() && !winget_ok {
                        r.known = false;
                    } else {
                        r.installed = installs::is_installed(a, &scan);
                        r.known = true;
                    }
                    source2.set_row_data(ri, r);
                }
            }
            if let Some(app) = weak.upgrade() {
                app.global::<Installer>().set_scanning(false);
            }
        }
        // Decode a few fetched icons per tick, spreading the work across frames.
        let mut budget = 16;
        while budget > 0 {
            let Ok((id, path)) = icon_rx.try_recv() else {
                break;
            };
            budget -= 1;
            if let Some(&ri) = id_to_row2.get(&id) {
                if let Some(img) = load_icon(&path) {
                    if let Some(mut r) = source2.row_data(ri) {
                        r.icon = img;
                        source2.set_row_data(ri, r);
                    }
                }
            }
        }
    });
    timer
}

/// First character of a name, uppercased, for the icon monogram fallback.
fn monogram_of(name: &str) -> String {
    name.chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default()
}

/// Decode a cached favicon into a Slint image. The DuckDuckGo service serves a
/// mix of formats under the `.ico` name (ICO, PNG, even WebP), so we detect the
/// format from the bytes rather than the extension. Returns None on a
/// missing/undecodable file, in which case the row keeps its monogram.
fn load_icon(path: &std::path::Path) -> Option<slint::Image> {
    let bytes = std::fs::read(path).ok()?;
    let img = decode_favicon(&bytes)?;
    let (w, h) = img.dimensions();
    let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(w, h);
    buf.make_mut_bytes().copy_from_slice(img.as_raw());
    Some(slint::Image::from_rgba8(buf))
}

fn decode_favicon(bytes: &[u8]) -> Option<image::RgbaImage> {
    let mut img = decode_favicon_bytes(bytes)?;
    // Some 32-bpp BMP icons carry a zero alpha channel (their transparency lived
    // in the ICO's separate AND mask, which the decoder drops), so they decode to
    // a fully transparent image. Treat all-zero alpha as opaque so it stays visible.
    if img.pixels().all(|p| p.0[3] == 0) {
        for p in img.pixels_mut() {
            p.0[3] = 255;
        }
    }
    Some(img)
}

fn decode_favicon_bytes(bytes: &[u8]) -> Option<image::RgbaImage> {
    if let Ok(img) = image::load_from_memory(bytes) {
        return Some(img.to_rgba8());
    }
    // Some ICOs embed a non-RGBA PNG that image's ICO decoder rejects. Find the
    // embedded PNG and decode it directly (the PNG decoder handles any colour type).
    const PNG_SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let pos = bytes.windows(PNG_SIG.len()).position(|w| w == PNG_SIG)?;
    image::load_from_memory_with_format(&bytes[pos..], image::ImageFormat::Png)
        .ok()
        .map(|img| img.to_rgba8())
}

// ── Config ──────────────────────────────────────────────────────────

fn wire_config(
    app: &AppWindow,
    jrnl: &SharedJournal,
    journal_path: &Path,
    notify: &Notify,
    tweaks_catalog: &Rc<Vec<tweaks::Tweak>>,
    tweaks_model: &Rc<VecModel<TweakRow>>,
    modes_catalog: &Rc<Vec<modes::Mode>>,
) -> Timer {
    let cfg_path = config::default_path();
    // Export runs off-thread (it scans winget for the app set); results come back
    // as (toml, tweak count, app count, mode) and land on the UI via the timer.
    let (etx, erx) = mpsc::channel::<(String, usize, usize, Option<String>)>();

    {
        let weak = app.as_weak();
        let cfg_path = cfg_path.clone();
        let etx = etx.clone();
        app.global::<Configuration>().on_export_config(move || {
            if let Some(app) = weak.upgrade() {
                app.global::<Configuration>()
                    .set_status("Capturing provisioning profile (scanning apps)…".into());
            }
            let cfg_path = cfg_path.clone();
            let etx = etx.clone();
            std::thread::spawn(move || {
                let cfg = config::capture_profile();
                let toml = cfg.to_toml().unwrap_or_default();
                let _ = std::fs::write(&cfg_path, &toml);
                let _ = etx.send((toml, cfg.tweaks.len(), cfg.apps.len(), cfg.mode));
            });
        });
    }

    {
        let weak = app.as_weak();
        let jrnl = jrnl.clone();
        let jpath = journal_path.to_path_buf();
        let notify = notify.clone();
        let tcat = tweaks_catalog.clone();
        let tmodel = tweaks_model.clone();
        let mcat = modes_catalog.clone();
        let cfg_path = cfg_path.clone();
        app.global::<Configuration>().on_import_config(move || {
            let toml = match std::fs::read_to_string(&cfg_path) {
                Ok(s) => s,
                Err(_) => {
                    notify("error", &format!("No config at {}", cfg_path.display()));
                    return;
                }
            };
            let cfg = match config::Config::from_toml(&toml) {
                Ok(cfg) => cfg,
                Err(e) => {
                    notify("error", &format!("Parse error: {e}"));
                    return;
                }
            };
            let applied = config::apply(&cfg, &mut jrnl.borrow_mut(), &jpath);
            if let Some(app) = weak.upgrade() {
                app.global::<Configuration>()
                    .set_preview(toml.as_str().into());
                refresh_tweaks(&tmodel, &tcat);
                refresh_modes(&app, &mcat);
            }
            notify(
                "success",
                &format!(
                    "Applied {} tweak action(s), mode {}",
                    applied,
                    cfg.mode.as_deref().unwrap_or("none")
                ),
            );
            // Provisioning: install the profile's app set in a visible elevated
            // console (winget). Security: an imported profile is untrusted, so only
            // ids that exist in our own catalog are ever installed (and the script
            // builder additionally rejects non-token ids). This blocks command
            // injection and arbitrary-package installs from a malicious profile.
            let known = installs::catalog();
            let apps: Vec<String> = cfg
                .apps
                .iter()
                .filter(|id| known.iter().any(|a| a.id == **id))
                .cloned()
                .collect();
            if !apps.is_empty() {
                match launch_elevated_ps(&installs::install_many_script(&apps), true) {
                    Ok(()) => notify(
                        "info",
                        &format!(
                            "Installing {} app(s) from the profile (approve UAC), see the console.",
                            apps.len()
                        ),
                    ),
                    Err(e) => notify("error", &format!("App install failed: {e}")),
                }
            }
        });
    }

    // Fixes, elevated repair commands run in a visible console.
    {
        let notify = notify.clone();
        app.global::<Configuration>().on_run_fix(move |idx| {
            let Some((name, script)) = repair::fixes().get(idx as usize) else {
                return;
            };
            match launch_elevated_ps(script, true) {
                Ok(()) => notify(
                    "info",
                    &format!("{name}, approve UAC; progress shows in the console."),
                ),
                Err(e) => notify("error", &format!("{name} failed: {e}")),
            }
        });
    }

    // Windows Update mode, elevated registry/service changes, run hidden.
    {
        let notify = notify.clone();
        app.global::<Configuration>()
            .on_set_update_mode(move |idx| {
                let Some((name, script)) = repair::update_modes().get(idx as usize) else {
                    return;
                };
                match launch_elevated_ps(script, false) {
                    Ok(()) => notify("success", &format!("Windows Update → {name} (approve UAC)")),
                    Err(e) => notify("error", &format!("{name} failed: {e}")),
                }
            });
    }

    // Restore points, create one (elevated) or open the Windows wizard.
    {
        let notify = notify.clone();
        app.global::<Configuration>()
            .on_create_restore_point(move || {
                let script = "Enable-ComputerRestore -Drive 'C:\\'; \
                Checkpoint-Computer -Description 'NeonPrime' -RestorePointType 'MODIFY_SETTINGS'; \
                Write-Host 'Restore point created.'";
                match launch_elevated_ps(script, true) {
                    Ok(()) => notify(
                        "info",
                        "Creating restore point, approve UAC; see the console.",
                    ),
                    Err(e) => notify("error", &format!("Restore point: {e}")),
                }
            });
    }
    {
        let notify = notify.clone();
        app.global::<Configuration>()
            .on_open_system_restore(move || match Command::new("rstrui.exe").spawn() {
                Ok(_) => notify("info", "Opening System Restore…"),
                Err(e) => notify("error", &format!("Couldn't open System Restore: {e}")),
            });
    }

    // Pump async export results back to the UI.
    let weak = app.as_weak();
    let cfg_path2 = cfg_path.clone();
    let notify2 = notify.clone();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(200), move || {
        while let Ok((toml, ntweaks, napps, mode)) = erx.try_recv() {
            if let Some(app) = weak.upgrade() {
                let c = app.global::<Configuration>();
                c.set_preview(toml.as_str().into());
                c.set_status(format!("Exported → {}", cfg_path2.display()).as_str().into());
            }
            notify2(
                "success",
                &format!(
                    "Exported {ntweaks} tweak(s), {napps} app(s), mode {}",
                    mode.as_deref().unwrap_or("none")
                ),
            );
        }
    });
    timer
}

// ── Theme + Undo ────────────────────────────────────────────────────

fn wire_theme(app: &AppWindow) {
    let weak = app.as_weak();
    app.global::<Ui>().on_set_theme(move |mode| {
        let Some(app) = weak.upgrade() else { return };
        app.global::<Theme>().set_mode(mode);
        // Load-modify-save so other settings (e.g. show_app_icons) are preserved.
        let mut s = settings::Settings::load();
        s.theme = mode;
        s.save();
    });
}

fn wire_undo(
    app: &AppWindow,
    jrnl: &SharedJournal,
    journal_path: &Path,
    notify: &Notify,
    tweaks_catalog: &Rc<Vec<tweaks::Tweak>>,
    tweaks_model: &Rc<VecModel<TweakRow>>,
    modes_catalog: &Rc<Vec<modes::Mode>>,
) {
    let weak = app.as_weak();
    let jrnl = jrnl.clone();
    let path = journal_path.to_path_buf();
    let notify = notify.clone();
    let tcat = tweaks_catalog.clone();
    let tmodel = tweaks_model.clone();
    let mcat = modes_catalog.clone();

    app.global::<Ui>().on_undo_last(move || {
        let entry = jrnl
            .borrow()
            .entries
            .iter()
            .rev()
            .find(|e| e.active)
            .cloned();
        let Some(entry) = entry else {
            notify("info", "Nothing to undo");
            return;
        };
        match engine::revert(&entry.reversal) {
            Ok(()) => {
                jrnl.borrow_mut().mark_reverted(entry.id);
                let _ = jrnl.borrow().save(&path);
                refresh_tweaks(&tmodel, &tcat);
                if let Some(app) = weak.upgrade() {
                    refresh_modes(&app, &mcat);
                }
                notify("success", &format!("Reverted: {}", entry.label));
            }
            Err(e) => notify("error", &format!("Undo failed: {e}")),
        }
    });
}

/// Launch a PowerShell script elevated via UAC (Start-Process -Verb RunAs).
/// `visible` keeps a `-NoExit` console open so the user can watch long-running
/// repairs (SFC/DISM); otherwise the elevated shell runs hidden and exits.
fn launch_elevated_ps(script: &str, visible: bool) -> io::Result<()> {
    let esc = script.replace('\'', "''");
    let inner = if visible {
        format!("'-NoExit','-Command','{esc}'")
    } else {
        format!("'-Command','{esc}'")
    };
    let hidden = if visible { "" } else { " -WindowStyle Hidden" };
    let ps =
        format!("Start-Process -FilePath 'powershell' -ArgumentList {inner} -Verb RunAs{hidden}");
    hidden_command("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps])
        .spawn()
        .map(|_| ())
}

/// Launch a `.ps1` file elevated in a visible console (`-NoExit`, RunAs). Used
/// for long scripts where nested -Command quoting would be fragile (MicroWin).
///
/// `prefer_pwsh` opens the console in PowerShell 7 (`pwsh`) when it is installed,
/// falling back to Windows PowerShell 5.1. The profile installer wants this so the
/// window it leaves open is the modern shell the profile actually targets, rather
/// than 5.1 (whose stock PSReadLine 2.0 lacks features the profile uses).
fn launch_elevated_file(ps1: &Path, prefer_pwsh: bool) -> io::Result<()> {
    let path = ps1.to_string_lossy().replace('\'', "''");
    // The path element must carry literal double quotes: Start-Process joins the
    // -ArgumentList array with spaces WITHOUT re-quoting, so a bare path in
    // "Program Files" would reach powershell as `-File C:\Program` and fail.
    let inner = format!("'-NoExit','-ExecutionPolicy','Bypass','-File','\"{path}\"'");
    let ps = if prefer_pwsh {
        format!(
            "$sh=if(Get-Command pwsh -ErrorAction SilentlyContinue){{'pwsh'}}else{{'powershell'}};\
             Start-Process -FilePath $sh -ArgumentList {inner} -Verb RunAs"
        )
    } else {
        format!("Start-Process -FilePath 'powershell' -ArgumentList {inner} -Verb RunAs")
    };
    hidden_command("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps])
        .spawn()
        .map(|_| ())
}

/// Launch a script in a visible, non-elevated PowerShell console (stays open).
fn launch_console(script: &str) -> io::Result<()> {
    Command::new("powershell")
        .args(["-NoExit", "-Command", script])
        .spawn()
        .map(|_| ())
}

fn wire_quick(app: &AppWindow, notify: &Notify) {
    let catalog = Rc::new(quick::catalog());
    let rows: Vec<QuickRow> = catalog
        .iter()
        .enumerate()
        .map(|(i, a)| QuickRow {
            id: i as i32,
            name: a.name.into(),
            desc: a.desc.into(),
            danger: a.danger,
            elevated: a.elevated,
        })
        .collect();
    app.global::<Quick>()
        .set_rows(Rc::new(VecModel::from(rows)).into());

    let cat = catalog.clone();
    let notify = notify.clone();
    app.global::<Quick>().on_run(move |id| {
        let Some(a) = cat.get(id as usize) else { return };

        // The PowerShell profile installer mirrors WinUtil: ensure Windows
        // Terminal + pwsh, then run the online setup in a wt/pwsh tab (see
        // quick::install_ps_profile_cmd for why it is fetched, not staged). The
        // launcher is hidden; the wt tab is the visible progress.
        if a.id == "install-ps-profile" {
            match launch_elevated_ps(&quick::install_ps_profile_cmd(), false) {
                Ok(()) => notify(
                    "info",
                    "Setting up the PowerShell profile in a new terminal tab (approve UAC)…",
                ),
                Err(e) => notify("error", &format!("Couldn't start installer: {e}")),
            }
            return;
        }

        // Removing the profile only edits the user's own $PROFILE, so no
        // elevation. A direct spawn passes the path as one argv, so a
        // Program-Files space is not a problem (unlike the Start-Process path).
        if a.id == "remove-ps-profile" {
            let mut script = std::env::current_exe().unwrap_or_default();
            script.pop();
            script.push("profile");
            script.push("uninstall-profile.ps1");
            match Command::new("powershell")
                .args([
                    "-NoExit",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    &script.to_string_lossy(),
                ])
                .spawn()
            {
                Ok(_) => notify("info", "Removing PowerShell profile, see the new window."),
                Err(e) => notify("error", &format!("Couldn't start remover: {e}")),
            }
            return;
        }

        let Some(inv) = quick::invocation(a.id) else { return };

        let result = if inv.elevated {
            // Launch elevated via UAC (Start-Process -Verb RunAs). Returns at once.
            // `visible` actions (SFC/DISM and other repairs) keep their console up so
            // the user can watch; the rest run hidden.
            let arglist = inv
                .args
                .iter()
                .map(|s| format!("'{}'", s.replace('\'', "''")))
                .collect::<Vec<_>>()
                .join(",");
            let window = if inv.visible { "" } else { " -WindowStyle Hidden" };
            let ps = format!(
                "Start-Process -FilePath '{}' -ArgumentList {arglist} -Verb RunAs{window}",
                inv.program
            );
            hidden_command("powershell")
                .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps])
                .spawn()
                .map(|_| ())
        } else {
            hidden_command(&inv.program).args(&inv.args).spawn().map(|_| ())
        };

        match result {
            Ok(()) => notify("info", &format!("Running: {}", a.name)),
            Err(e) => notify("error", &format!("{} failed: {e}", a.name)),
        }
    });
}

fn wire_startup(app: &AppWindow, notify: &Notify) -> Rc<dyn Fn()> {
    let model: Rc<VecModel<StartupRow>> = Rc::new(VecModel::default());

    let rebuild = {
        let model = model.clone();
        Rc::new(move || {
            let rows: Vec<StartupRow> = startup::list()
                .into_iter()
                .enumerate()
                .map(|(i, e)| StartupRow {
                    id: i as i32,
                    name: e.name.as_str().into(),
                    command: e.command.as_str().into(),
                    enabled: e.enabled,
                })
                .collect();
            model.set_vec(rows);
        })
    };
    rebuild();
    app.global::<Startup>().set_rows(model.clone().into());

    let notify = notify.clone();
    let rebuild2 = rebuild.clone();
    let model2 = model.clone();
    app.global::<Startup>().on_toggle(move |id, want| {
        if let Some(row) = model2.row_data(id as usize) {
            let (name, cmd) = (row.name.to_string(), row.command.to_string());
            let res = if want {
                startup::enable(&name, &cmd)
            } else {
                startup::disable(&name, &cmd)
            };
            match res {
                Ok(()) => notify(
                    "success",
                    &format!("{} {}", name, if want { "enabled" } else { "disabled" }),
                ),
                Err(e) => notify("error", &format!("{name}: {e}")),
            }
        }
        rebuild2();
    });

    rebuild
}

/// Windows optional features: enable/disable via elevated DISM in a visible
/// console. Enable/disable itself needs admin, but each row shows a best-effort
/// current state detected UNELEVATED (file/registry probes), so the user isn't
/// guessing. State refreshes when the panel is navigated to. Returns that
/// refresh closure. DISM changes usually need a reboot, so the state reflects the
/// last boot until then.
fn wire_features(app: &AppWindow, notify: &Notify) -> Rc<dyn Fn()> {
    fn rows() -> Vec<FeatureRow> {
        features::catalog()
            .iter()
            .enumerate()
            .map(|(i, f)| FeatureRow {
                id: i as i32,
                name: f.name.into(),
                desc: f.desc.into(),
                state: features::detect_state(f.id).code(),
            })
            .collect()
    }
    let model: Rc<VecModel<FeatureRow>> = Rc::new(VecModel::from(rows()));
    app.global::<Features>().set_rows(model.clone().into());

    let notify = notify.clone();
    app.global::<Features>().on_apply(move |id, enable| {
        let Some(f) = features::catalog().get(id as usize) else {
            return;
        };
        let script = features::dism_script(f, enable);
        let verb = if enable { "Enabling" } else { "Disabling" };
        match launch_elevated_ps(&script, true) {
            Ok(()) => notify(
                "info",
                &format!(
                    "{verb} {}, approve UAC; DISM progress shows in the console.",
                    f.name
                ),
            ),
            Err(e) => notify("error", &format!("{}: {e}", f.name)),
        }
    });

    Rc::new(move || model.set_vec(rows()))
}

/// UWP debloat: probe installed packages off-thread (unelevated), remove per-user,
/// and disable telemetry scheduled tasks (elevated). Returns the result pump.
fn wire_debloat(app: &AppWindow, notify: &Notify) -> Timer {
    let model: Rc<VecModel<DebloatRow>> = Rc::new(VecModel::default());
    let rows: Vec<DebloatRow> = debloat::catalog()
        .iter()
        .enumerate()
        .map(|(i, b)| DebloatRow {
            id: i as i32,
            name: b.name.into(),
            desc: b.desc.into(),
            present: false,
            known: false,
        })
        .collect();
    model.set_vec(rows);
    app.global::<Debloat>().set_rows(model.clone().into());
    app.global::<Debloat>().set_probing(true);

    let (tx, rx) = mpsc::channel::<DebloatMsg>();

    // Probe installed packages off-thread (Get-AppxPackage is slow).
    {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(DebloatMsg::Probed(debloat::installed_names()));
        });
    }

    // Remove one package (per-user, unelevated) on a worker thread.
    {
        let notify = notify.clone();
        let tx = tx.clone();
        app.global::<Debloat>().on_remove(move |id| {
            let Some(b) = debloat::catalog().get(id as usize) else {
                return;
            };
            notify("info", &format!("Removing {}…", b.name));
            let (tx, name) = (tx.clone(), b.name.to_string());
            std::thread::spawn(move || {
                let b = &debloat::catalog()[id as usize];
                let (ok, err) = match debloat::remove(b) {
                    Ok(o) => (o, String::new()),
                    Err(e) => (false, e.to_string()),
                };
                let _ = tx.send(DebloatMsg::Removed {
                    idx: id,
                    ok,
                    name,
                    err,
                });
            });
        });
    }

    // Disable telemetry scheduled tasks (elevated, hidden console).
    {
        let notify = notify.clone();
        app.global::<Debloat>().on_disable_telemetry_tasks(move || {
            match launch_elevated_ps(&debloat::disable_tasks_script(), false) {
                Ok(()) => notify(
                    "info",
                    "Disabling telemetry tasks, approve the UAC prompt…",
                ),
                Err(e) => notify("error", &format!("Failed: {e}")),
            }
        });
    }

    // Pump: apply probe + removal results.
    let weak = app.as_weak();
    let model2 = model.clone();
    let notify2 = notify.clone();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(200), move || {
        while let Ok(msg) = rx.try_recv() {
            match msg {
                DebloatMsg::Probed(set) => {
                    for (i, b) in debloat::catalog().iter().enumerate() {
                        if let Some(mut row) = model2.row_data(i) {
                            row.present = debloat::is_present(b, &set);
                            row.known = true;
                            model2.set_row_data(i, row);
                        }
                    }
                    if let Some(app) = weak.upgrade() {
                        app.global::<Debloat>().set_probing(false);
                    }
                }
                DebloatMsg::Removed { idx, ok, name, err } => {
                    if ok {
                        if let Some(mut row) = model2.row_data(idx as usize) {
                            row.present = false;
                            model2.set_row_data(idx as usize, row);
                        }
                        notify2("success", &format!("Removed: {name}"));
                    } else if err.is_empty() {
                        notify2(
                            "error",
                            &format!("{name}: removal blocked (system/provisioned app)"),
                        );
                    } else {
                        notify2("error", &format!("{name}: {err}"));
                    }
                }
            }
        }
    });
    timer
}

/// Power-plan switcher (Modes panel). Reads the active scheme unelevated and
/// switches it via elevated `powercfg`. Returns a refresh closure for nav.
fn wire_power(app: &AppWindow, notify: &Notify) -> Rc<dyn Fn()> {
    let refresh: Rc<dyn Fn()> = {
        let weak = app.as_weak();
        Rc::new(move || {
            if let Some(app) = weak.upgrade() {
                app.global::<Power>().set_active_plan(power::active_index());
            }
        })
    };
    refresh();

    let notify = notify.clone();
    app.global::<Power>().on_set_plan(move |idx| {
        let Some(script) = power::set_script(idx as usize) else {
            return;
        };
        let name = power::plans()
            .get(idx as usize)
            .map(|p| p.name)
            .unwrap_or("plan");
        match launch_elevated_ps(&script, false) {
            Ok(()) => notify(
                "info",
                &format!("Switching to {name}, approve the UAC prompt…"),
            ),
            Err(e) => notify("error", &format!("Power plan failed: {e}")),
        }
    });
    refresh
}

/// Disk cleanup: scan reclaimable sizes off-thread, clean user targets in-process
/// and system caches via an elevated shell. Returns the result pump.
fn wire_cleanup(app: &AppWindow, notify: &Notify) -> Timer {
    // The panel is a flat one-row-per-option list over the whole catalog
    // (built-in system targets plus detected browsers). Both the catalog and its
    // flattened rows are rebuilt cheaply from pure producers wherever needed
    // (including worker threads), so nothing but plain sizes crosses the thread
    // boundary. A row's `id` is its index in `cleaners::rows`.
    let count = cleaners::rows(&cleaners::catalog()).len();
    let model: Rc<VecModel<CleanRow>> = Rc::new(VecModel::default());
    let rows: Vec<CleanRow> = cleaners::rows(&cleaners::catalog())
        .iter()
        .enumerate()
        .map(|(i, r)| CleanRow {
            id: i as i32,
            name: r.name.as_str().into(),
            desc: r.desc.as_str().into(),
            size: "-".into(),
            frac: 0.0,
            elevated: r.elevated,
            warning: r.warning.as_str().into(),
            imported: r.imported,
        })
        .collect();
    model.set_vec(rows);
    app.global::<Cleanup>().set_rows(model.clone().into());
    app.global::<Cleanup>().set_scanning(true);

    let sizes: Rc<RefCell<Vec<u64>>> = Rc::new(RefCell::new(vec![0; count]));
    let (tx, rx) = mpsc::channel::<CleanMsg>();

    let scan = {
        let tx = tx.clone();
        move || {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let cat = cleaners::catalog();
                let v: Vec<u64> = cleaners::rows(&cat)
                    .iter()
                    .map(|r| {
                        let c = &cat[r.cleaner];
                        cleaners::preview(c, &cleaners::only(c.options.len(), r.option)).bytes
                    })
                    .collect();
                let _ = tx.send(CleanMsg::Scanned(v));
            });
        }
    };
    scan();

    // Rescan button.
    {
        let weak = app.as_weak();
        let scan = scan.clone();
        app.global::<Cleanup>().on_rescan(move || {
            if let Some(app) = weak.upgrade() {
                app.global::<Cleanup>().set_scanning(true);
            }
            scan();
        });
    }

    // Import button: (re)load winapp2.ini from the app data dir, then rescan so
    // detected cleaners appear. The file itself is untrusted; parsing only ever
    // produces sandboxed file-cleaning actions (no registry deletes).
    {
        let weak = app.as_weak();
        let scan = scan.clone();
        let notify = notify.clone();
        app.global::<Cleanup>().on_import(move || {
            let path = cleaners::winapp2_path();
            if !path.exists() {
                notify(
                    "info",
                    &format!("Drop a winapp2.ini at {} then IMPORT.", path.display()),
                );
                return;
            }
            cleaners::invalidate_import();
            let n = cleaners::imported_cleaners().len();
            if let Some(app) = weak.upgrade() {
                app.global::<Cleanup>().set_scanning(true);
            }
            scan();
            notify("success", &format!("Imported {n} winapp2 cleaner(s)."));
        });
    }

    // Clean one target.
    {
        let notify = notify.clone();
        let tx = tx.clone();
        app.global::<Cleanup>().on_clean(move |idx| {
            let cat = cleaners::catalog();
            let all_rows = cleaners::rows(&cat);
            let Some(r) = all_rows.get(idx as usize) else {
                return;
            };
            let c = &cat[r.cleaner];
            let name = r.name.clone();
            let sel = cleaners::only(c.options.len(), r.option);
            if r.elevated {
                if let Some(script) = cleaners::elevated_script(c, &sel) {
                    match launch_elevated_ps(&script, false) {
                        Ok(()) => notify(
                            "info",
                            &format!("Clearing {name}, approve UAC, then RESCAN."),
                        ),
                        Err(e) => notify("error", &format!("{name}: {e}")),
                    }
                }
            } else if r.guard_running && cleaners::any_running(&r.running_procs) {
                // Deleting a live browser profile's cookies/history would corrupt
                // it, so destructive options are hard-blocked while it is open.
                notify(
                    "error",
                    &format!("Close {} first to clean {name}.", c.name),
                );
            } else {
                notify("info", &format!("Cleaning {name}…"));
                let tx = tx.clone();
                std::thread::spawn(move || {
                    let cat = cleaners::catalog();
                    let rows = cleaners::rows(&cat);
                    let size = if let Some(r) = rows.get(idx as usize) {
                        let c = &cat[r.cleaner];
                        let sel = cleaners::only(c.options.len(), r.option);
                        cleaners::execute(c, &sel, false);
                        cleaners::preview(c, &sel).bytes
                    } else {
                        0
                    };
                    let _ = tx.send(CleanMsg::Cleaned { idx, size, name });
                });
            }
        });
    }

    // Rebuild the visible list from current sizes: largest target first, each row
    // carrying its share of the biggest so the bar shows relative weight.
    let rebuild: Rc<dyn Fn()> = {
        let weak = app.as_weak();
        let model = model.clone();
        let sizes = sizes.clone();
        Rc::new(move || {
            let sizes = sizes.borrow();
            let max = sizes.iter().copied().max().unwrap_or(0).max(1);
            let mut rows: Vec<CleanRow> = cleaners::rows(&cleaners::catalog())
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    let sz = sizes.get(i).copied().unwrap_or(0);
                    CleanRow {
                        id: i as i32,
                        name: r.name.as_str().into(),
                        desc: r.desc.as_str().into(),
                        size: cleaners::human(sz).into(),
                        frac: sz as f32 / max as f32,
                        elevated: r.elevated,
                        warning: r.warning.as_str().into(),
                        imported: r.imported,
                    }
                })
                .collect();
            rows.sort_by(|a, b| {
                let sa = sizes.get(a.id as usize).copied().unwrap_or(0);
                let sb = sizes.get(b.id as usize).copied().unwrap_or(0);
                sb.cmp(&sa)
            });
            model.set_vec(rows);
            if let Some(app) = weak.upgrade() {
                let total: u64 = sizes.iter().sum();
                app.global::<Cleanup>()
                    .set_total(cleaners::human(total).into());
            }
        })
    };

    // Pump.
    let weak = app.as_weak();
    let sizes2 = sizes.clone();
    let notify2 = notify.clone();
    let rebuild2 = rebuild.clone();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(200), move || {
        let mut dirty = false;
        while let Ok(msg) = rx.try_recv() {
            match msg {
                CleanMsg::Scanned(v) => {
                    *sizes2.borrow_mut() = v;
                    if let Some(app) = weak.upgrade() {
                        app.global::<Cleanup>().set_scanning(false);
                    }
                    dirty = true;
                }
                CleanMsg::Cleaned { idx, size, name } => {
                    if let Some(s) = sizes2.borrow_mut().get_mut(idx as usize) {
                        *s = size;
                    }
                    notify2("success", &format!("Cleaned: {name}"));
                    dirty = true;
                }
            }
        }
        if dirty {
            rebuild2();
        }
    });
    timer
}

/// Process & resource monitor, top processes by CPU with per-process GPU/VRAM,
/// plus a kill action. Returns a refresh closure (nav + telemetry-tick driven).
fn wire_proc(app: &AppWindow, notify: &Notify) -> Rc<dyn Fn()> {
    let model: Rc<VecModel<ProcRow>> = Rc::new(VecModel::default());
    app.global::<Procs>().set_rows(model.clone().into());
    let monitor = Rc::new(RefCell::new(procmon::ProcMonitor::new()));
    // Last raw sample, so sort/filter can re-render without re-sampling the system.
    let last: Rc<RefCell<Vec<procmon::Proc>>> = Rc::new(RefCell::new(Vec::new()));

    // Apply the current filter + sort key to the last snapshot.
    let apply: Rc<dyn Fn()> = {
        let weak = app.as_weak();
        let model = model.clone();
        let last = last.clone();
        Rc::new(move || {
            let Some(app) = weak.upgrade() else { return };
            let q = app.global::<Procs>().get_filter_text().to_lowercase();
            let key = app.global::<Procs>().get_sort_key();
            let src = last.borrow();
            let mut procs: Vec<&procmon::Proc> = src
                .iter()
                .filter(|p| q.is_empty() || p.name.to_lowercase().contains(&q))
                .collect();
            let cmp_f = |x: f32, y: f32| y.partial_cmp(&x).unwrap_or(std::cmp::Ordering::Equal);
            match key {
                1 => procs.sort_by_key(|p| std::cmp::Reverse(p.mem)),
                2 => procs.sort_by(|a, b| cmp_f(a.gpu, b.gpu)),
                3 => procs.sort_by_key(|p| std::cmp::Reverse(p.vram)),
                4 => procs.sort_by_key(|p| p.name.to_lowercase()),
                _ => procs.sort_by(|a, b| cmp_f(a.cpu, b.cpu)),
            }
            procs.truncate(60);
            let rows: Vec<ProcRow> = procs
                .iter()
                .map(|p| {
                    let gpu = if p.gpu >= 0.5 {
                        format!("{:.0}%", p.gpu)
                    } else {
                        "-".into()
                    };
                    let vram = if p.vram > 0 {
                        cleaners::human(p.vram)
                    } else {
                        "-".into()
                    };
                    ProcRow {
                        pid: p.pid as i32,
                        name: p.name.as_str().into(),
                        cpu: format!("{:.0}%", p.cpu).as_str().into(),
                        mem: cleaners::human(p.mem).as_str().into(),
                        gpu: gpu.as_str().into(),
                        vram: vram.as_str().into(),
                    }
                })
                .collect();
            let n = rows.len() as i32;
            model.set_vec(rows);
            app.global::<Procs>().set_count(n);
        })
    };

    let refresh: Rc<dyn Fn()> = {
        let monitor = monitor.clone();
        let last = last.clone();
        let apply = apply.clone();
        Rc::new(move || {
            let snap = monitor.borrow_mut().snapshot(300);
            *last.borrow_mut() = snap;
            apply();
        })
    };
    refresh();
    {
        let refresh = refresh.clone();
        app.global::<Procs>().on_refresh(move || refresh());
    }
    {
        let apply = apply.clone();
        app.global::<Procs>().on_reapply(move || apply());
    }
    {
        let notify = notify.clone();
        let refresh = refresh.clone();
        app.global::<Procs>().on_kill(move |pid| {
            if procmon::kill(pid as u32) {
                notify("success", &format!("Killed pid {pid}"));
            } else {
                notify("error", &format!("Couldn't kill pid {pid} (protected?)"));
            }
            refresh();
        });
    }
    refresh
}

/// Services manager, list (unelevated, off-thread) with search; start/stop and
/// start-type changes go through the elevated shell. Returns the load pump.
fn wire_services(app: &AppWindow, notify: &Notify) -> Timer {
    let source: Rc<VecModel<ServiceRow>> = Rc::new(VecModel::default());
    let filter_text = Rc::new(RefCell::new(String::new()));
    let filtered = Rc::new(FilterModel::new(ModelRc::from(source.clone()), {
        let ft = filter_text.clone();
        move |row: &ServiceRow| {
            let q = ft.borrow();
            q.is_empty()
                || row.display.to_lowercase().contains(q.as_str())
                || row.name.to_lowercase().contains(q.as_str())
        }
    }));
    app.global::<Services>()
        .set_rows(ModelRc::from(filtered.clone()));
    app.global::<Services>().set_scanning(true);

    {
        let weak = app.as_weak();
        let ft = filter_text.clone();
        let filtered = filtered.clone();
        app.global::<Services>().on_filter(move || {
            if let Some(app) = weak.upgrade() {
                *ft.borrow_mut() = app.global::<Services>().get_filter_text().to_lowercase();
                filtered.reset();
            }
        });
    }

    let (tx, rx) = mpsc::channel::<Vec<services::Svc>>();
    let scan = {
        let tx = tx.clone();
        move || {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(services::list());
            });
        }
    };
    scan();

    {
        let weak = app.as_weak();
        let scan = scan.clone();
        app.global::<Services>().on_refresh(move || {
            if let Some(app) = weak.upgrade() {
                app.global::<Services>().set_scanning(true);
            }
            scan();
        });
    }

    // Elevated start / stop / start-type.
    {
        let notify = notify.clone();
        app.global::<Services>().on_start(move |name| {
            match launch_elevated_ps(&services::start_script(&name), false) {
                Ok(()) => notify(
                    "info",
                    &format!("Starting {name} (approve UAC), then REFRESH"),
                ),
                Err(e) => notify("error", &format!("{name}: {e}")),
            }
        });
    }
    {
        let notify = notify.clone();
        app.global::<Services>().on_stop(move |name| {
            match launch_elevated_ps(&services::stop_script(&name), false) {
                Ok(()) => notify(
                    "info",
                    &format!("Stopping {name} (approve UAC), then REFRESH"),
                ),
                Err(e) => notify("error", &format!("{name}: {e}")),
            }
        });
    }
    {
        let notify = notify.clone();
        app.global::<Services>().on_set_startup(move |name, code| {
            match launch_elevated_ps(&services::startup_script(&name, code), false) {
                Ok(()) => notify(
                    "info",
                    &format!("{name}: start-type change (approve UAC), then REFRESH"),
                ),
                Err(e) => notify("error", &format!("{name}: {e}")),
            }
        });
    }

    // Pump: populate the source model when the scan completes.
    let weak = app.as_weak();
    let source2 = source.clone();
    let filtered2 = filtered.clone();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(200), move || {
        while let Ok(svcs) = rx.try_recv() {
            let rows: Vec<ServiceRow> = svcs
                .iter()
                .map(|s| ServiceRow {
                    name: s.name.as_str().into(),
                    display: s.display.as_str().into(),
                    running: s.running,
                    startup: s.startup as i32,
                })
                .collect();
            source2.set_vec(rows);
            filtered2.reset();
            if let Some(app) = weak.upgrade() {
                app.global::<Services>().set_scanning(false);
            }
        }
    });
    timer
}

/// Event Viewer: recent System/Application errors and warnings, filtered by level
/// and text. Read-only, scanned off-thread like Services.
fn wire_events(app: &AppWindow) -> Timer {
    let source: Rc<VecModel<EventRow>> = Rc::new(VecModel::default());
    // (lowercased search text, level filter): 0 all, 1 warnings+errors, 2 errors.
    let filter_state = Rc::new(RefCell::new((String::new(), 0i32)));
    let filtered = Rc::new(FilterModel::new(ModelRc::from(source.clone()), {
        let st = filter_state.clone();
        move |row: &EventRow| {
            let (q, lvl) = &*st.borrow();
            let level_ok = match lvl {
                2 => row.level == 2,
                1 => row.level >= 1,
                _ => true,
            };
            let text_ok = q.is_empty()
                || row.source.to_lowercase().contains(q.as_str())
                || row.message.to_lowercase().contains(q.as_str());
            level_ok && text_ok
        }
    }));
    app.global::<Events>()
        .set_rows(ModelRc::from(filtered.clone()));
    app.global::<Events>().set_loading(true);

    {
        let weak = app.as_weak();
        let st = filter_state.clone();
        let filtered = filtered.clone();
        app.global::<Events>().on_filter(move || {
            if let Some(app) = weak.upgrade() {
                let ev = app.global::<Events>();
                *st.borrow_mut() = (ev.get_filter_text().to_lowercase(), ev.get_level_filter());
                filtered.reset();
            }
        });
    }

    let (tx, rx) = mpsc::channel::<Vec<eventlog::EventEntry>>();
    let scan = {
        let tx = tx.clone();
        move || {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(eventlog::recent(300));
            });
        }
    };
    scan();

    {
        let weak = app.as_weak();
        let scan = scan.clone();
        app.global::<Events>().on_refresh(move || {
            if let Some(app) = weak.upgrade() {
                app.global::<Events>().set_loading(true);
            }
            scan();
        });
    }

    let weak = app.as_weak();
    let source2 = source.clone();
    let filtered2 = filtered.clone();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(200), move || {
        while let Ok(events) = rx.try_recv() {
            let rows: Vec<EventRow> = events
                .iter()
                .map(|e| EventRow {
                    time: e.time.as_str().into(),
                    level: e.level as i32,
                    source: e.source.as_str().into(),
                    id: e.id as i32,
                    log: e.log.as_str().into(),
                    message: e.message.as_str().into(),
                })
                .collect();
            source2.set_vec(rows);
            filtered2.reset();
            if let Some(app) = weak.upgrade() {
                app.global::<Events>().set_loading(false);
            }
        }
    });
    timer
}

/// Local Users panel: list accounts + Administrators membership (unelevated), with
/// elevated enable/disable, admin toggle, password-never-expires, and a `net user`
/// password reset in a visible console.
fn wire_users(app: &AppWindow, notify: &Notify) -> Timer {
    let source: Rc<VecModel<LocalUser>> = Rc::new(VecModel::default());
    let filter_text = Rc::new(RefCell::new(String::new()));
    let filtered = Rc::new(FilterModel::new(ModelRc::from(source.clone()), {
        let ft = filter_text.clone();
        move |row: &LocalUser| {
            let q = ft.borrow();
            q.is_empty()
                || row.name.to_lowercase().contains(q.as_str())
                || row.full_name.to_lowercase().contains(q.as_str())
        }
    }));
    app.global::<Users>()
        .set_rows(ModelRc::from(filtered.clone()));
    app.global::<Users>().set_loading(true);

    {
        let weak = app.as_weak();
        let ft = filter_text.clone();
        let filtered = filtered.clone();
        app.global::<Users>().on_filter(move || {
            if let Some(app) = weak.upgrade() {
                *ft.borrow_mut() = app.global::<Users>().get_filter_text().to_lowercase();
                filtered.reset();
            }
        });
    }

    let (tx, rx) = mpsc::channel::<Vec<localusers::LocalUser>>();
    let scan = {
        let tx = tx.clone();
        move || {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(localusers::list());
            });
        }
    };
    scan();

    {
        let weak = app.as_weak();
        let scan = scan.clone();
        app.global::<Users>().on_refresh(move || {
            if let Some(app) = weak.upgrade() {
                app.global::<Users>().set_loading(true);
            }
            scan();
        });
    }

    // Elevated account changes (approve UAC, then REFRESH to see the result).
    {
        let notify = notify.clone();
        app.global::<Users>().on_set_enabled(move |name, want| {
            let verb = if want { "Enabling" } else { "Disabling" };
            match launch_elevated_ps(&localusers::enable_script(&name, want), false) {
                Ok(()) => notify("info", &format!("{verb} {name} (approve UAC), then REFRESH")),
                Err(e) => notify("error", &format!("{name}: {e}")),
            }
        });
    }
    {
        let notify = notify.clone();
        app.global::<Users>().on_set_admin(move |name, want| {
            let verb = if want { "Granting admin to" } else { "Removing admin from" };
            match launch_elevated_ps(&localusers::admin_script(&name, want), false) {
                Ok(()) => notify("info", &format!("{verb} {name} (approve UAC), then REFRESH")),
                Err(e) => notify("error", &format!("{name}: {e}")),
            }
        });
    }
    {
        let notify = notify.clone();
        app.global::<Users>().on_set_expiry(move |name, never| {
            match launch_elevated_ps(&localusers::expiry_script(&name, never), false) {
                Ok(()) => notify(
                    "info",
                    &format!("Updating password expiry for {name} (approve UAC), then REFRESH"),
                ),
                Err(e) => notify("error", &format!("{name}: {e}")),
            }
        });
    }
    {
        let notify = notify.clone();
        app.global::<Users>().on_reset_password(move |name| {
            match launch_elevated_ps(&localusers::reset_password_script(&name), true) {
                Ok(()) => notify(
                    "info",
                    &format!("Resetting {name} password (approve UAC), then type it in the console."),
                ),
                Err(e) => notify("error", &format!("{name}: {e}")),
            }
        });
    }

    let weak = app.as_weak();
    let source2 = source.clone();
    let filtered2 = filtered.clone();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(200), move || {
        while let Ok(users) = rx.try_recv() {
            let rows: Vec<LocalUser> = users
                .iter()
                .map(|u| LocalUser {
                    name: u.name.as_str().into(),
                    full_name: u.full_name.as_str().into(),
                    description: u.description.as_str().into(),
                    enabled: u.enabled,
                    is_admin: u.is_admin,
                    never_expires: u.never_expires,
                })
                .collect();
            source2.set_vec(rows);
            filtered2.reset();
            if let Some(app) = weak.upgrade() {
                app.global::<Users>().set_loading(false);
            }
        }
    });
    timer
}

/// A local timestamp string for reports (one cheap PowerShell call).
fn ps_now() -> String {
    hidden_command("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "(Get-Date).ToString('yyyy-MM-dd HH:mm')",
        ])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Write the compliance report to the user's profile folder and open it.
fn write_compliance_report(items: &[posture::PostureItem]) -> io::Result<String> {
    let machine = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "this-pc".into());
    let html = posture::report_html(items, &machine, &ps_now());
    let mut path = PathBuf::from(std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into()));
    path.push(format!("neonprime-compliance-{machine}.html"));
    std::fs::write(&path, html)?;
    let p = path.to_string_lossy().to_string();
    let _ = Command::new("explorer").arg(&p).spawn();
    Ok(p)
}

/// Compliance & posture panel: read-only security board (Defender, firewall,
/// BitLocker, TPM, Secure Boot, UAC, update age) with an HTML report export.
fn wire_posture(app: &AppWindow, notify: &Notify) -> Timer {
    let source: Rc<VecModel<PostureRow>> = Rc::new(VecModel::default());
    app.global::<Posture>()
        .set_rows(ModelRc::from(source.clone()));
    app.global::<Posture>().set_loading(true);

    let (tx, rx) = mpsc::channel::<Vec<posture::PostureItem>>();
    let scan = {
        let tx = tx.clone();
        move || {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(posture::scan());
            });
        }
    };
    scan();

    {
        let weak = app.as_weak();
        let scan = scan.clone();
        app.global::<Posture>().on_refresh(move || {
            if let Some(app) = weak.upgrade() {
                app.global::<Posture>().set_loading(true);
            }
            scan();
        });
    }

    {
        let notify = notify.clone();
        let src = source.clone();
        app.global::<Posture>().on_export_report(move || {
            let mut items = Vec::with_capacity(src.row_count());
            for i in 0..src.row_count() {
                if let Some(r) = src.row_data(i) {
                    items.push(posture::PostureItem {
                        name: r.name.to_string(),
                        status: r.status.to_string(),
                        state: r.state.clamp(0, 3) as u8,
                        detail: r.detail.to_string(),
                    });
                }
            }
            match write_compliance_report(&items) {
                Ok(p) => notify("info", &format!("Saved compliance report: {p}")),
                Err(e) => notify("error", &format!("Export failed: {e}")),
            }
        });
    }

    let weak = app.as_weak();
    let source2 = source.clone();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(200), move || {
        while let Ok(items) = rx.try_recv() {
            let (g, w, b) = posture::summary(&items);
            let rows: Vec<PostureRow> = items
                .iter()
                .map(|i| PostureRow {
                    name: i.name.as_str().into(),
                    status: i.status.as_str().into(),
                    state: i.state as i32,
                    detail: i.detail.as_str().into(),
                })
                .collect();
            source2.set_vec(rows);
            if let Some(app) = weak.upgrade() {
                let p = app.global::<Posture>();
                p.set_good(g as i32);
                p.set_warn(w as i32);
                p.set_bad(b as i32);
                p.set_loading(false);
            }
        }
    });
    timer
}

/// Support Bundle panel: asset identity + warranty link, and a one-click machine
/// snapshot written to a timestamped folder (generated off-thread).
fn wire_support(app: &AppWindow, notify: &Notify) -> Timer {
    // Warranty URL kept for the button once the asset scan lands.
    let warranty = Rc::new(RefCell::new(String::new()));

    let (atx, arx) = mpsc::channel::<asset::AssetInfo>();
    std::thread::spawn(move || {
        let _ = atx.send(asset::info());
    });

    let (btx, brx) = mpsc::channel::<Result<bundle::BundleResult, String>>();

    {
        let weak = app.as_weak();
        let btx = btx.clone();
        app.global::<Support>().on_generate(move || {
            if let Some(app) = weak.upgrade() {
                app.global::<Support>().set_generating(true);
            }
            let btx = btx.clone();
            std::thread::spawn(move || {
                let _ = btx.send(bundle::generate());
            });
        });
    }
    {
        let weak = app.as_weak();
        app.global::<Support>().on_open_folder(move || {
            if let Some(app) = weak.upgrade() {
                let p = app.global::<Support>().get_last_path().to_string();
                if !p.is_empty() {
                    let _ = Command::new("explorer").arg(&p).spawn();
                }
            }
        });
    }
    {
        let warranty = warranty.clone();
        app.global::<Support>().on_warranty_lookup(move || {
            let url = warranty.borrow().clone();
            if !url.is_empty() {
                let _ = Command::new("explorer").arg(&url).spawn();
            }
        });
    }

    let weak = app.as_weak();
    let warranty2 = warranty.clone();
    let notify = notify.clone();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(200), move || {
        if let Ok(a) = arx.try_recv() {
            *warranty2.borrow_mut() = a.warranty_url.clone();
            if let Some(app) = weak.upgrade() {
                let s = app.global::<Support>();
                s.set_asset_manufacturer(a.manufacturer.as_str().into());
                s.set_asset_model(a.model.as_str().into());
                s.set_asset_serial(a.serial.as_str().into());
                s.set_has_warranty(!a.warranty_url.is_empty());
            }
        }
        while let Ok(res) = brx.try_recv() {
            if let Some(app) = weak.upgrade() {
                let s = app.global::<Support>();
                s.set_generating(false);
                match res {
                    Ok(b) => {
                        s.set_last_path(b.path.as_str().into());
                        notify("info", &format!("Support bundle ready ({} files).", b.files));
                    }
                    Err(e) => notify("error", &format!("Bundle failed: {e}")),
                }
            }
        }
    });
    timer
}

/// Printers panel: list printers + queue depth (unelevated), clear a queue or
/// restart the spooler (elevated).
fn wire_printers(app: &AppWindow, notify: &Notify) -> Timer {
    let source: Rc<VecModel<PrinterRow>> = Rc::new(VecModel::default());
    app.global::<Printers>()
        .set_rows(ModelRc::from(source.clone()));
    app.global::<Printers>().set_loading(true);

    let (tx, rx) = mpsc::channel::<Vec<printers::Printer>>();
    let scan = {
        let tx = tx.clone();
        move || {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(printers::list());
            });
        }
    };
    scan();
    {
        let weak = app.as_weak();
        let scan = scan.clone();
        app.global::<Printers>().on_refresh(move || {
            if let Some(app) = weak.upgrade() {
                app.global::<Printers>().set_loading(true);
            }
            scan();
        });
    }
    {
        let notify = notify.clone();
        app.global::<Printers>().on_clear_queue(move |name| {
            match launch_elevated_ps(&printers::clear_queue_script(&name), false) {
                Ok(()) => notify("info", &format!("Clearing {name} queue (approve UAC), then REFRESH")),
                Err(e) => notify("error", &format!("{name}: {e}")),
            }
        });
    }
    {
        let notify = notify.clone();
        app.global::<Printers>().on_restart_spooler(move || {
            match launch_elevated_ps(&printers::restart_spooler_script(), false) {
                Ok(()) => notify("info", "Restarting Print Spooler (approve UAC), then REFRESH"),
                Err(e) => notify("error", &format!("Spooler: {e}")),
            }
        });
    }

    let weak = app.as_weak();
    let source2 = source.clone();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(200), move || {
        while let Ok(v) = rx.try_recv() {
            let rows: Vec<PrinterRow> = v
                .iter()
                .map(|p| PrinterRow {
                    name: p.name.as_str().into(),
                    status: p.status.as_str().into(),
                    jobs: p.jobs as i32,
                    is_default: p.is_default,
                })
                .collect();
            source2.set_vec(rows);
            if let Some(app) = weak.upgrade() {
                app.global::<Printers>().set_loading(false);
            }
        }
    });
    timer
}

/// Profiles panel: local user profiles with size + last-use (unelevated), delete
/// a stale profile (elevated).
fn wire_profiles(app: &AppWindow, notify: &Notify) -> Timer {
    let source: Rc<VecModel<ProfileRow>> = Rc::new(VecModel::default());
    app.global::<Profiles>()
        .set_rows(ModelRc::from(source.clone()));
    app.global::<Profiles>().set_loading(true);

    let (tx, rx) = mpsc::channel::<Vec<profiles::Profile>>();
    let scan = {
        let tx = tx.clone();
        move || {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(profiles::list());
            });
        }
    };
    scan();
    {
        let weak = app.as_weak();
        let scan = scan.clone();
        app.global::<Profiles>().on_refresh(move || {
            if let Some(app) = weak.upgrade() {
                app.global::<Profiles>().set_loading(true);
            }
            scan();
        });
    }
    {
        let notify = notify.clone();
        app.global::<Profiles>().on_delete_profile(move |sid| {
            match launch_elevated_ps(&profiles::delete_script(&sid), false) {
                Ok(()) => notify("info", "Removing profile (approve UAC), then REFRESH"),
                Err(e) => notify("error", &format!("Profile: {e}")),
            }
        });
    }

    let weak = app.as_weak();
    let source2 = source.clone();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(200), move || {
        while let Ok(v) = rx.try_recv() {
            let rows: Vec<ProfileRow> = v
                .iter()
                .map(|p| ProfileRow {
                    account: p.account.as_str().into(),
                    path: p.path.as_str().into(),
                    size_mb: p.size_mb.clamp(-1, i32::MAX as i64) as i32,
                    last_use: p.last_use.as_str().into(),
                    loaded: p.loaded,
                    sid: p.sid.as_str().into(),
                })
                .collect();
            source2.set_vec(rows);
            if let Some(app) = weak.upgrade() {
                app.global::<Profiles>().set_loading(false);
            }
        }
    });
    timer
}

/// Write a text report to the user's profile folder and open it.
fn write_text_report(name: &str, content: &str) -> io::Result<String> {
    let mut path = PathBuf::from(std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into()));
    path.push(name);
    std::fs::write(&path, content)?;
    let p = path.to_string_lossy().to_string();
    let _ = Command::new("explorer").arg(&p).spawn();
    Ok(p)
}

/// Disks panel: physical-disk health + per-volume free space (unelevated).
fn wire_disks(app: &AppWindow) -> Timer {
    let vol_src: Rc<VecModel<VolumeRow>> = Rc::new(VecModel::default());
    let phys_src: Rc<VecModel<PhysRow>> = Rc::new(VecModel::default());
    app.global::<Disks>()
        .set_volumes(ModelRc::from(vol_src.clone()));
    app.global::<Disks>()
        .set_physical(ModelRc::from(phys_src.clone()));
    app.global::<Disks>().set_loading(true);

    let (tx, rx) = mpsc::channel::<(Vec<disks::PhysDisk>, Vec<disks::Volume>)>();
    let scan = {
        let tx = tx.clone();
        move || {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send((disks::physical(), disks::volumes()));
            });
        }
    };
    scan();
    {
        let weak = app.as_weak();
        let scan = scan.clone();
        app.global::<Disks>().on_refresh(move || {
            if let Some(app) = weak.upgrade() {
                app.global::<Disks>().set_loading(true);
            }
            scan();
        });
    }

    let weak = app.as_weak();
    let vs = vol_src.clone();
    let ps = phys_src.clone();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(200), move || {
        while let Ok((phys, vols)) = rx.try_recv() {
            let prows: Vec<PhysRow> = phys
                .iter()
                .map(|d| PhysRow {
                    model: d.model.as_str().into(),
                    media: d.media.as_str().into(),
                    size_gb: d.size_gb.clamp(0, i32::MAX as i64) as i32,
                    health: d.health.as_str().into(),
                    state: d.state as i32,
                })
                .collect();
            let vrows: Vec<VolumeRow> = vols
                .iter()
                .map(|v| VolumeRow {
                    name: v.name.as_str().into(),
                    label: v.label.as_str().into(),
                    fs: v.fs.as_str().into(),
                    total_gb: v.total_gb.clamp(0, i32::MAX as i64) as i32,
                    free_gb: v.free_gb.clamp(0, i32::MAX as i64) as i32,
                    used_frac: v.used_frac,
                })
                .collect();
            ps.set_vec(prows);
            vs.set_vec(vrows);
            if let Some(app) = weak.upgrade() {
                app.global::<Disks>().set_loading(false);
            }
        }
    });
    timer
}

/// Drivers panel: signed-driver inventory with problem-device flagging, text
/// filter, a problems-only toggle, and an export (unelevated).
fn wire_devices(app: &AppWindow, notify: &Notify) -> Timer {
    let source: Rc<VecModel<DeviceRow>> = Rc::new(VecModel::default());
    // (lowercased text, problems-only).
    let state = Rc::new(RefCell::new((String::new(), false)));
    let filtered = Rc::new(FilterModel::new(ModelRc::from(source.clone()), {
        let st = state.clone();
        move |row: &DeviceRow| {
            let (q, po) = &*st.borrow();
            let prob_ok = !po || row.problem;
            let text_ok = q.is_empty()
                || row.name.to_lowercase().contains(q.as_str())
                || row.class.to_lowercase().contains(q.as_str());
            prob_ok && text_ok
        }
    }));
    app.global::<Devices>()
        .set_rows(ModelRc::from(filtered.clone()));
    app.global::<Devices>().set_loading(true);

    {
        let weak = app.as_weak();
        let st = state.clone();
        let filtered = filtered.clone();
        app.global::<Devices>().on_filter(move || {
            if let Some(app) = weak.upgrade() {
                let d = app.global::<Devices>();
                *st.borrow_mut() = (d.get_filter_text().to_lowercase(), d.get_problems_only());
                filtered.reset();
            }
        });
    }

    let raw = Rc::new(RefCell::new(Vec::<devices::Device>::new()));
    {
        let raw = raw.clone();
        let notify = notify.clone();
        app.global::<Devices>().on_export_list(move || {
            let text = devices::to_text(&raw.borrow());
            match write_text_report("neonprime-drivers.txt", &text) {
                Ok(p) => notify("info", &format!("Saved driver inventory: {p}")),
                Err(e) => notify("error", &format!("Export failed: {e}")),
            }
        });
    }

    let (tx, rx) = mpsc::channel::<Vec<devices::Device>>();
    let scan = {
        let tx = tx.clone();
        move || {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(devices::list());
            });
        }
    };
    scan();
    {
        let weak = app.as_weak();
        let scan = scan.clone();
        app.global::<Devices>().on_refresh(move || {
            if let Some(app) = weak.upgrade() {
                app.global::<Devices>().set_loading(true);
            }
            scan();
        });
    }

    let weak = app.as_weak();
    let source2 = source.clone();
    let filtered2 = filtered.clone();
    let raw2 = raw.clone();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(200), move || {
        while let Ok(devs) = rx.try_recv() {
            let problems = devs.iter().filter(|d| d.problem).count();
            let total = devs.len();
            let rows: Vec<DeviceRow> = devs
                .iter()
                .map(|d| DeviceRow {
                    name: d.name.as_str().into(),
                    class: d.class.as_str().into(),
                    version: d.version.as_str().into(),
                    date: d.date.as_str().into(),
                    problem: d.problem,
                })
                .collect();
            *raw2.borrow_mut() = devs;
            source2.set_vec(rows);
            filtered2.reset();
            if let Some(app) = weak.upgrade() {
                let d = app.global::<Devices>();
                d.set_total(total as i32);
                d.set_problems(problems as i32);
                d.set_loading(false);
            }
        }
    });
    timer
}

/// Certificates panel: machine-store certs by soonest expiry (unelevated).
fn wire_certs(app: &AppWindow) -> Timer {
    let source: Rc<VecModel<CertRow>> = Rc::new(VecModel::default());
    app.global::<Certs>()
        .set_rows(ModelRc::from(source.clone()));
    app.global::<Certs>().set_loading(true);

    let (tx, rx) = mpsc::channel::<Vec<certs::Cert>>();
    let scan = {
        let tx = tx.clone();
        move || {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(certs::list());
            });
        }
    };
    scan();
    {
        let weak = app.as_weak();
        let scan = scan.clone();
        app.global::<Certs>().on_refresh(move || {
            if let Some(app) = weak.upgrade() {
                app.global::<Certs>().set_loading(true);
            }
            scan();
        });
    }

    let weak = app.as_weak();
    let source2 = source.clone();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(200), move || {
        while let Ok(v) = rx.try_recv() {
            let rows: Vec<CertRow> = v
                .iter()
                .map(|c| CertRow {
                    subject: c.subject.as_str().into(),
                    issuer: c.issuer.as_str().into(),
                    expires: c.expires.as_str().into(),
                    days_left: c.days_left.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
                    state: c.state as i32,
                })
                .collect();
            source2.set_vec(rows);
            if let Some(app) = weak.upgrade() {
                app.global::<Certs>().set_loading(false);
            }
        }
    });
    timer
}

/// Group Policy (RSoP) panel: applied GPOs + last refresh via gpresult, plus a
/// full HTML report export.
fn wire_gpo(app: &AppWindow, notify: &Notify) -> Timer {
    let applied: Rc<VecModel<slint::SharedString>> = Rc::new(VecModel::default());
    app.global::<Gpo>()
        .set_applied(ModelRc::from(applied.clone()));
    app.global::<Gpo>().set_loading(true);

    let (tx, rx) = mpsc::channel::<gpo::GpoInfo>();
    let scan = {
        let tx = tx.clone();
        move || {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(gpo::info());
            });
        }
    };
    scan();
    {
        let weak = app.as_weak();
        let scan = scan.clone();
        app.global::<Gpo>().on_refresh(move || {
            if let Some(app) = weak.upgrade() {
                app.global::<Gpo>().set_loading(true);
            }
            scan();
        });
    }
    {
        let notify = notify.clone();
        app.global::<Gpo>().on_export_report(move || {
            notify("info", "Generating the RSoP report…");
            std::thread::spawn(move || {
                let mut path =
                    PathBuf::from(std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into()));
                path.push("neonprime-gpreport.html");
                let p = path.to_string_lossy().to_string();
                let _ = hidden_command("gpresult").args(gpo::export_argv(&p)).status();
                let _ = Command::new("explorer").arg(&p).spawn();
            });
        });
    }

    let weak = app.as_weak();
    let applied2 = applied.clone();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(200), move || {
        while let Ok(info) = rx.try_recv() {
            let names: Vec<slint::SharedString> =
                info.applied.iter().map(|s| s.as_str().into()).collect();
            applied2.set_vec(names);
            if let Some(app) = weak.upgrade() {
                let g = app.global::<Gpo>();
                g.set_last_refresh(info.last_refresh.as_str().into());
                g.set_loading(false);
            }
        }
    });
    timer
}

/// MicroWin, debloated-ISO builder. Generates an elevated build script + an
/// autounattend, then runs them in a visible console. (Heavy, admin, ~20 GB.)
fn wire_microwin(app: &AppWindow, notify: &Notify) {
    let osc = microwin::oscdimg_path();
    {
        let m = app.global::<MicroWin>();
        m.set_oscdimg_ok(osc.is_some());
        m.set_oscdimg_hint(match &osc {
            Some(p) => format!("oscdimg ready, {p}").as_str().into(),
            None => {
                "oscdimg NOT found, install the Windows ADK 'Deployment Tools' to build.".into()
            }
        });
    }

    // Default the output path once the source ISO is set.
    {
        let weak = app.as_weak();
        app.global::<MicroWin>().on_iso_edited(move || {
            if let Some(app) = weak.upgrade() {
                let m = app.global::<MicroWin>();
                let iso = m.get_iso().to_string();
                if m.get_output().is_empty() && !iso.trim().is_empty() {
                    m.set_output(microwin::default_output(&iso).as_str().into());
                }
            }
        });
    }

    {
        let weak = app.as_weak();
        let notify = notify.clone();
        app.global::<MicroWin>().on_build(move || {
            let Some(app) = weak.upgrade() else { return };
            let m = app.global::<MicroWin>();
            let iso = m.get_iso().to_string();
            if iso.trim().is_empty() || !Path::new(&iso).exists() {
                notify("error", "Source ISO not found, check the path.");
                return;
            }
            let Some(oscdimg) = microwin::oscdimg_path() else {
                notify(
                    "error",
                    "oscdimg not found, install the Windows ADK Deployment Tools.",
                );
                return;
            };
            let output = {
                let o = m.get_output().to_string();
                if o.trim().is_empty() {
                    microwin::default_output(&iso)
                } else {
                    o
                }
            };
            let index = m.get_index().to_string().trim().parse::<u32>().unwrap_or(1);
            let opts = microwin::Options {
                iso,
                output,
                scratch: microwin::default_scratch(),
                index,
                debloat: m.get_debloat(),
                privacy: m.get_privacy(),
                bypass: m.get_bypass(),
            };
            let tmp = std::env::temp_dir();
            let unattend = tmp.join("neonprime-unattend.xml");
            if opts.bypass {
                let _ = std::fs::write(&unattend, microwin::AUTOUNATTEND);
            }
            let script = microwin::build_script(&opts, &oscdimg, &unattend.to_string_lossy());
            let ps1 = tmp.join("neonprime-microwin.ps1");
            if std::fs::write(&ps1, script).is_err() {
                notify("error", "Couldn't write the build script.");
                return;
            }
            match launch_elevated_file(&ps1, false) {
                Ok(()) => notify(
                    "info",
                    "MicroWin started, approve UAC; the build runs in the console (10+ min).",
                ),
                Err(e) => notify("error", &format!("MicroWin: {e}")),
            }
        });
    }
}

/// Network monitor, snapshot active outbound TCP connections per process.
/// Returns a refresh closure (driven by nav + the telemetry tick while visible).
fn wire_network(app: &AppWindow, notify: &Notify) -> Rc<dyn Fn()> {
    let model: Rc<VecModel<NetRow>> = Rc::new(VecModel::default());
    app.global::<Network>().set_rows(model.clone().into());
    let fw_model: Rc<VecModel<slint::SharedString>> = Rc::new(VecModel::default());
    app.global::<Network>()
        .set_fw_rules(fw_model.clone().into());

    let resolver = netmon::Resolver::new();
    let refresh: Rc<dyn Fn()> = {
        let weak = app.as_weak();
        let model = model.clone();
        let resolver = resolver.clone();
        Rc::new(move || {
            let rows: Vec<NetRow> = netmon::connections()
                .iter()
                .map(|c| NetRow {
                    proc_name: c.proc_name.as_str().into(),
                    pid: c.pid as i32,
                    remote: c.remote.as_str().into(),
                    host: resolver.host(c.remote_ip).as_str().into(),
                    state: c.state.as_str().into(),
                    path: c.path.as_str().into(),
                })
                .collect();
            let n = rows.len() as i32;
            model.set_vec(rows);
            if let Some(app) = weak.upgrade() {
                app.global::<Network>().set_count(n);
            }
        })
    };
    refresh();

    // Reload NeonPrime's firewall block rules (unelevated read).
    let refresh_fw: Rc<dyn Fn()> = {
        let fw_model = fw_model.clone();
        Rc::new(move || {
            let names: Vec<slint::SharedString> = firewall::list_names()
                .iter()
                .map(|n| n.as_str().into())
                .collect();
            fw_model.set_vec(names);
        })
    };

    {
        let refresh = refresh.clone();
        app.global::<Network>().on_refresh(move || refresh());
    }
    {
        let refresh_fw = refresh_fw.clone();
        app.global::<Network>()
            .on_refresh_firewall(move || refresh_fw());
    }
    {
        let notify = notify.clone();
        app.global::<Network>().on_set_dns(move |idx| {
            let Some(script) = dns::set_script(idx as usize) else {
                return;
            };
            let name = dns::providers()
                .get(idx as usize)
                .map(|p| p.name)
                .unwrap_or("DNS");
            match launch_elevated_ps(&script, false) {
                Ok(()) => notify("info", &format!("Setting DNS → {name} (approve UAC)")),
                Err(e) => notify("error", &format!("DNS: {e}")),
            }
        });
    }
    {
        let notify = notify.clone();
        app.global::<Network>().on_block_app(move |name, path| {
            let Some(script) = firewall::block_script(&name, &path) else {
                notify(
                    "error",
                    "No executable path for that process, can't block.",
                );
                return;
            };
            match launch_elevated_ps(&script, false) {
                Ok(()) => notify("info", &format!("Blocking {name} outbound (approve UAC)")),
                Err(e) => notify("error", &format!("Firewall: {e}")),
            }
        });
    }
    {
        let notify = notify.clone();
        let refresh_fw = refresh_fw.clone();
        app.global::<Network>().on_unblock(move |name| {
            match launch_elevated_ps(&firewall::unblock_script(&name), false) {
                Ok(()) => notify("info", &format!("Removing rule: {name} (approve UAC)")),
                Err(e) => notify("error", &format!("Firewall: {e}")),
            }
            refresh_fw();
        });
    }
    refresh
}

/// Subsequence fuzzy match. Returns None when `needle` is not a subsequence of
/// `hay`, otherwise a score that rewards contiguous runs, a prefix hit, and a
/// straight substring hit. Case-insensitive.
fn fuzzy_score(hay: &str, needle: &str) -> Option<i32> {
    if needle.is_empty() {
        return Some(0);
    }
    let hay_l = hay.to_lowercase();
    let needle_l = needle.to_lowercase();
    let mut chars = hay_l.chars();
    let mut score = 0i32;
    let mut streak = 0i32;
    for nc in needle_l.chars() {
        loop {
            match chars.next() {
                Some(hc) if hc == nc => {
                    streak += 1;
                    score += 1 + streak;
                    break;
                }
                Some(_) => streak = 0,
                None => return None,
            }
        }
    }
    if hay_l.starts_with(&needle_l) {
        score += 20;
    } else if hay_l.contains(&needle_l) {
        score += 10;
    }
    Some(score)
}

/// Command palette (Ctrl+K): fuzzy list of panels to jump to + actions to run.
/// id encodes the target, `<1000` nav page, `1000+` quick action, `2000+` mode.
fn wire_palette(app: &AppWindow) {
    const NAV: &[(&str, i32)] = &[
        ("Dashboard", 0),
        ("Network", 11),
        ("Tweaks", 1),
        ("Privacy", 8),
        ("Debloat", 10),
        ("Cleanup", 12),
        ("Startup", 6),
        ("Install", 2),
        ("Features", 7),
        ("Modes", 3),
        ("Actions", 5),
        ("Config", 4),
        ("History", 9),
    ];
    let mut cmds: Vec<(String, &'static str, i32)> = Vec::new();
    for (label, page) in NAV {
        cmds.push((format!("Go to {label}"), "Panel", *page));
    }
    for (i, a) in quick::catalog().iter().enumerate() {
        cmds.push((format!("Run: {}", a.name), "Action", 1000 + i as i32));
    }
    for (i, m) in modes::catalog().iter().enumerate() {
        cmds.push((format!("Activate {} mode", m.name), "Mode", 2000 + i as i32));
    }
    let cmds = Rc::new(cmds);
    // Recently-run command ids, most-recent first (session-scoped).
    let recents: Rc<RefCell<Vec<i32>>> = Rc::new(RefCell::new(Vec::new()));

    let model: Rc<VecModel<PaletteItem>> = Rc::new(VecModel::default());
    app.global::<Palette>().set_items(model.clone().into());

    {
        let weak = app.as_weak();
        let cmds = cmds.clone();
        let model = model.clone();
        let recents = recents.clone();
        app.global::<Palette>().on_filter(move || {
            let q = weak
                .upgrade()
                .map(|a| a.global::<Palette>().get_query().to_lowercase())
                .unwrap_or_default();
            let to_item = |(l, h, id): &(String, &'static str, i32)| PaletteItem {
                label: l.as_str().into(),
                hint: (*h).into(),
                id: *id,
            };
            let items: Vec<PaletteItem> = if q.is_empty() {
                // Empty query: recents first (recency order), then the rest.
                let rec = recents.borrow();
                let mut ordered: Vec<&(String, &'static str, i32)> = rec
                    .iter()
                    .filter_map(|id| cmds.iter().find(|(_, _, cid)| cid == id))
                    .collect();
                ordered.extend(cmds.iter().filter(|(_, _, id)| !rec.contains(id)));
                ordered.into_iter().take(50).map(to_item).collect()
            } else {
                // Fuzzy: keep subsequence matches, best score first.
                let mut scored: Vec<(i32, &(String, &'static str, i32))> = cmds
                    .iter()
                    .filter_map(|c| fuzzy_score(&c.0, &q).map(|s| (s, c)))
                    .collect();
                scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1 .0.len().cmp(&b.1 .0.len())));
                scored.into_iter().take(50).map(|(_, c)| to_item(c)).collect()
            };
            model.set_vec(items);
        });
    }
    app.global::<Palette>().invoke_filter();

    {
        let weak = app.as_weak();
        let recents = recents.clone();
        app.global::<Palette>().on_run(move |id| {
            {
                let mut rec = recents.borrow_mut();
                rec.retain(|&x| x != id);
                rec.insert(0, id);
                rec.truncate(8);
            }
            let Some(app) = weak.upgrade() else { return };
            if id >= 2000 {
                app.global::<Modes>().invoke_activate(id - 2000);
            } else if id >= 1000 {
                app.global::<Quick>().invoke_run(id - 1000);
            } else {
                app.global::<Nav>().set_page(id);
                app.global::<Nav>().invoke_changed(id);
            }
        });
    }
}

/// Privacy/Hardening score, a view over the tweak catalog. Reads live state to
/// score exposure (no elevation needed just to view), and hardens via the same
/// reversible apply path as the Tweaks panel. Returns the elevated-result pump.
fn wire_privacy(
    app: &AppWindow,
    jrnl: &SharedJournal,
    journal_path: &Path,
    notify: &Notify,
    tweaks_catalog: &Rc<Vec<tweaks::Tweak>>,
    tweaks_model: &Rc<VecModel<TweakRow>>,
) -> (Timer, Rc<dyn Fn()>) {
    // Resolve each privacy check id to its catalog index, once.
    let indices: Rc<Vec<usize>> = Rc::new(
        privacy::check_ids()
            .iter()
            .filter_map(|id| tweaks_catalog.iter().position(|t| t.id == *id))
            .collect(),
    );
    let model: Rc<VecModel<PrivacyCheck>> = Rc::new(VecModel::default());

    let broker: Arc<Mutex<Option<BrokerSession>>> = Arc::new(Mutex::new(None));
    let (tx, rx) = mpsc::channel::<ElevatedMsg>();

    // Re-probe every check from live registry state and recompute the score.
    let refresh: Rc<dyn Fn()> = {
        let weak = app.as_weak();
        let model = model.clone();
        let cat = tweaks_catalog.clone();
        let indices = indices.clone();
        Rc::new(move || {
            let mut hardened = 0i32;
            let rows: Vec<PrivacyCheck> = indices
                .iter()
                .map(|&i| {
                    let t = &cat[i];
                    let on = t.is_applied();
                    if on {
                        hardened += 1;
                    }
                    PrivacyCheck {
                        id: i as i32,
                        name: t.name.into(),
                        desc: t.desc.into(),
                        warn: t.warn.into(),
                        hardened: on,
                        elevated: t.needs_elevation(),
                    }
                })
                .collect();
            let total = rows.len() as i32;
            model.set_vec(rows);
            if let Some(app) = weak.upgrade() {
                let p = app.global::<Privacy>();
                p.set_hardened_count(hardened);
                p.set_total(total);
                p.set_score(if total > 0 { hardened * 100 / total } else { 0 });
            }
        })
    };
    refresh();
    app.global::<Privacy>().set_checks(model.clone().into());

    // Harden a single check (id == catalog index).
    {
        let cat = tweaks_catalog.clone();
        let jrnl = jrnl.clone();
        let path = journal_path.to_path_buf();
        let notify = notify.clone();
        let broker = broker.clone();
        let tx = tx.clone();
        let refresh = refresh.clone();
        app.global::<Privacy>().on_harden(move |id| {
            let Some(t) = cat.get(id as usize) else {
                return;
            };
            if t.needs_elevation() {
                notify("info", "Requesting elevation, approve the UAC prompt…");
                let (broker, tx, name, on) =
                    (broker.clone(), tx.clone(), t.name.to_string(), t.on.clone());
                std::thread::spawn(move || elevated_worker(broker, tx, on, id, name, true));
            } else {
                let _ = run_local(&t.on, &jrnl, t, true);
                let _ = jrnl.borrow().save(&path);
                refresh();
                notify("success", &format!("Hardened: {}", t.name));
            }
        });
    }

    // Harden every currently-exposed check in one go.
    {
        let cat = tweaks_catalog.clone();
        let indices = indices.clone();
        let jrnl = jrnl.clone();
        let path = journal_path.to_path_buf();
        let notify = notify.clone();
        let broker = broker.clone();
        let tx = tx.clone();
        let refresh = refresh.clone();
        app.global::<Privacy>().on_harden_all(move || {
            let (mut local, mut elevated) = (0, 0);
            for &i in indices.iter() {
                let t = &cat[i];
                if t.is_applied() {
                    continue;
                }
                if t.needs_elevation() {
                    elevated += 1;
                    let (broker, tx, name, on) =
                        (broker.clone(), tx.clone(), t.name.to_string(), t.on.clone());
                    std::thread::spawn(move || {
                        elevated_worker(broker, tx, on, i as i32, name, true)
                    });
                } else if run_local(&t.on, &jrnl, t, true).is_ok() {
                    local += 1;
                }
            }
            let _ = jrnl.borrow().save(&path);
            refresh();
            if elevated > 0 {
                notify(
                    "info",
                    &format!("Hardened {local} now; approve UAC for {elevated} more…"),
                );
            } else {
                notify("success", &format!("Hardened {local} checks"));
            }
        });
    }

    // Pump: journal + refresh once elevated hardening completes.
    let cat = tweaks_catalog.clone();
    let jrnl = jrnl.clone();
    let path = journal_path.to_path_buf();
    let notify = notify.clone();
    let tmodel = tweaks_model.clone();
    let refresh_pump = refresh.clone();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(150), move || {
        while let Ok(msg) = rx.try_recv() {
            match msg {
                ElevatedMsg::Done {
                    name,
                    want,
                    results,
                    ..
                } => {
                    {
                        let mut j = jrnl.borrow_mut();
                        for (a, rev) in results {
                            j.record(
                                format!("{}: {}", name, if want { "on" } else { "off" }),
                                a,
                                rev,
                            );
                        }
                    }
                    let _ = jrnl.borrow().save(&path);
                    refresh_pump();
                    refresh_tweaks(&tmodel, &cat);
                    notify("success", &format!("Hardened: {name}"));
                }
                ElevatedMsg::Failed { name, error, .. } => {
                    refresh_pump();
                    notify("error", &format!("{name}: {error}"));
                }
            }
        }
    });
    (timer, refresh)
}

/// Relative-time label for the history panel (e.g. "5m ago").
fn rel_time(ts: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let d = now.saturating_sub(ts);
    if d < 60 {
        "just now".into()
    } else if d < 3600 {
        format!("{}m ago", d / 60)
    } else if d < 86_400 {
        format!("{}h ago", d / 3600)
    } else {
        format!("{}d ago", d / 86_400)
    }
}

/// Worker-thread body for an elevated *revert* via the broker.
fn revert_elevated_worker(
    broker: Arc<Mutex<Option<BrokerSession>>>,
    tx: mpsc::Sender<RevertMsg>,
    reversal: Reversal,
    id: u64,
    label: String,
) {
    let mut guard = broker.lock().unwrap();
    if guard.is_none() {
        match BrokerSession::spawn(true) {
            Ok(s) => *guard = Some(s),
            Err(e) => {
                let _ = tx.send(RevertMsg::Failed {
                    label,
                    error: format!("elevation failed: {e}"),
                });
                return;
            }
        }
    }
    let session = guard.as_mut().unwrap();
    match session.client.call(&Request::Revert { reversal }) {
        Ok(Response::Reverted) => {
            let _ = tx.send(RevertMsg::Done { id, label });
        }
        Ok(Response::Error(e)) => {
            let _ = tx.send(RevertMsg::Failed { label, error: e });
        }
        Ok(_) => {
            let _ = tx.send(RevertMsg::Failed {
                label,
                error: "unexpected broker reply".into(),
            });
        }
        Err(e) => {
            *guard = None;
            let _ = tx.send(RevertMsg::Failed {
                label,
                error: format!("broker link lost: {e}"),
            });
        }
    }
}

/// History timeline + selective rollback over the journal. Reverts HKCU entries
/// locally and HKLM entries through the broker. Returns (pump timer, refresh fn).
fn wire_history(
    app: &AppWindow,
    jrnl: &SharedJournal,
    journal_path: &Path,
    notify: &Notify,
    tweaks_catalog: &Rc<Vec<tweaks::Tweak>>,
    tweaks_model: &Rc<VecModel<TweakRow>>,
) -> (Timer, Rc<dyn Fn()>) {
    let model: Rc<VecModel<HistoryRow>> = Rc::new(VecModel::default());
    let broker: Arc<Mutex<Option<BrokerSession>>> = Arc::new(Mutex::new(None));
    let (tx, rx) = mpsc::channel::<RevertMsg>();

    // Rebuild the timeline (newest first) from the journal.
    let refresh: Rc<dyn Fn()> = {
        let weak = app.as_weak();
        let model = model.clone();
        let jrnl = jrnl.clone();
        Rc::new(move || {
            let j = jrnl.borrow();
            let rows: Vec<HistoryRow> = j
                .entries
                .iter()
                .rev()
                .map(|e| HistoryRow {
                    id: e.id as i32,
                    label: e.label.as_str().into(),
                    when: rel_time(e.ts).into(),
                    detail: e.reversal.target_summary().into(),
                    active: e.active,
                    elevated: e.reversal.needs_elevation(),
                })
                .collect();
            let active = j.entries.iter().filter(|e| e.active).count() as i32;
            drop(j);
            model.set_vec(rows);
            if let Some(app) = weak.upgrade() {
                app.global::<History>().set_active_count(active);
            }
        })
    };
    refresh();
    app.global::<History>().set_rows(model.clone().into());

    // Revert one entry by id.
    let do_revert = {
        let jrnl = jrnl.clone();
        let path = journal_path.to_path_buf();
        let notify = notify.clone();
        let broker = broker.clone();
        let tx = tx.clone();
        let refresh = refresh.clone();
        Rc::new(move |id: u64| {
            let entry = jrnl.borrow().get(id).filter(|e| e.active).cloned();
            let Some(entry) = entry else { return };
            if entry.reversal.needs_elevation() {
                notify("info", "Requesting elevation, approve the UAC prompt…");
                let (broker, tx, rev, label) = (
                    broker.clone(),
                    tx.clone(),
                    entry.reversal.clone(),
                    entry.label.clone(),
                );
                std::thread::spawn(move || revert_elevated_worker(broker, tx, rev, id, label));
            } else {
                match engine::revert(&entry.reversal) {
                    Ok(()) => {
                        jrnl.borrow_mut().mark_reverted(id);
                        let _ = jrnl.borrow().save(&path);
                        refresh();
                        notify("success", &format!("Reverted: {}", entry.label));
                    }
                    Err(e) => notify("error", &format!("Revert failed: {e}")),
                }
            }
        })
    };

    {
        let do_revert = do_revert.clone();
        app.global::<History>()
            .on_revert(move |id| do_revert(id as u64));
    }

    // Revert every active entry, newest first.
    {
        let jrnl = jrnl.clone();
        let do_revert = do_revert.clone();
        app.global::<History>().on_revert_all(move || {
            let ids: Vec<u64> = jrnl
                .borrow()
                .entries
                .iter()
                .rev()
                .filter(|e| e.active)
                .map(|e| e.id)
                .collect();
            for id in ids {
                do_revert(id);
            }
        });
    }

    // Pump elevated-revert results.
    let jrnl = jrnl.clone();
    let path = journal_path.to_path_buf();
    let notify = notify.clone();
    let tcat = tweaks_catalog.clone();
    let tmodel = tweaks_model.clone();
    let refresh2 = refresh.clone();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(150), move || {
        while let Ok(msg) = rx.try_recv() {
            match msg {
                RevertMsg::Done { id, label } => {
                    jrnl.borrow_mut().mark_reverted(id);
                    let _ = jrnl.borrow().save(&path);
                    refresh2();
                    refresh_tweaks(&tmodel, &tcat);
                    notify("success", &format!("Reverted: {label}"));
                }
                RevertMsg::Failed { label, error } => {
                    notify("error", &format!("{label}: {error}"));
                }
            }
        }
    });
    (timer, refresh)
}

/// Print a line to the console that launched us and exit-friendly flush. This is
/// a windows-subsystem (GUI) binary with no console of its own, so we attach to
/// the parent process's console; when launched from a shell that captures stdout,
/// the inherited pipe already carries it. Lets `--version` / `--help` produce
/// real output instead of silently opening a window (what winget validation and
/// CLI users expect).
fn console_print(msg: &str) {
    use std::io::Write;
    // SAFETY: AttachConsole is a benign attach-to-parent; failure (no console) is
    // ignored, in which case the stdout write below is a harmless no-op.
    unsafe {
        use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{msg}");
    let _ = out.flush();
}

fn main() -> Result<(), slint::PlatformError> {
    // CLI flags, handled before any window is created. `neonprime.exe --version`
    // must print and exit, not launch the GUI (which produces no console output
    // and fails to open on a machine with no display/GPU, e.g. a validation
    // sandbox, looking like a broken binary).
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version" || a == "-V" || a == "-v") {
        console_print(&format!("NeonPrime {}", env!("CARGO_PKG_VERSION")));
        return Ok(());
    }
    if args.iter().any(|a| a == "--help" || a == "-h" || a == "/?") {
        console_print(&format!(
            "NeonPrime {}\n\
             A holographic system control deck (graphical app for Windows).\n\n\
             Usage:\n  \
             neonprime              Open the app.\n  \
             neonprime --version    Print the version and exit.\n  \
             neonprime --help       Show this help.",
            env!("CARGO_PKG_VERSION")
        ));
        return Ok(());
    }

    // Single-instance guard, a second launch exits rather than racing the journal.
    let instance = single_instance::SingleInstance::new("neonprime-singleton").ok();
    if let Some(inst) = &instance {
        if !inst.is_single() {
            return Ok(());
        }
    }

    let app = AppWindow::new()?;
    let notify = make_notifier(&app);

    app.global::<Theme>()
        .set_mode(settings::Settings::load().theme);

    let journal_path: PathBuf = journal::default_path();
    let jrnl: SharedJournal = Rc::new(RefCell::new(Journal::load(&journal_path)));

    // Tweaks model.
    let tweaks_catalog = Rc::new(tweaks::catalog());
    let rows: Vec<TweakRow> = tweaks_catalog
        .iter()
        .enumerate()
        .map(|(i, t)| make_row(i, t))
        .collect();
    let tweaks_model = Rc::new(VecModel::from(rows));
    // Tweaks.rows is set inside wire_tweaks (wrapped in a FilterModel).

    // Modes model.
    let modes_catalog = Rc::new(modes::catalog());
    let cards: Vec<ModeCard> = modes_catalog
        .iter()
        .enumerate()
        .map(|(i, m)| ModeCard {
            id: i as i32,
            name: m.name.into(),
            tagline: m.tagline.into(),
            desc: m.desc.into(),
        })
        .collect();
    app.global::<Modes>()
        .set_cards(Rc::new(VecModel::from(cards)).into());
    refresh_modes(&app, &modes_catalog);

    app.global::<Build>()
        .set_version(env!("CARGO_PKG_VERSION").into());
    wire_theme(&app);
    let _tweak_pump = wire_tweaks(
        &app,
        &jrnl,
        &journal_path,
        &notify,
        &tweaks_catalog,
        &tweaks_model,
    );
    wire_modes(&app, &jrnl, &journal_path, &notify, &modes_catalog);
    let _installs_pump = wire_installs(&app, &notify);
    wire_quick(&app, &notify);
    let startup_refresh = wire_startup(&app, &notify);
    let features_refresh = wire_features(&app, &notify);
    let _debloat_pump = wire_debloat(&app, &notify);
    let _cleanup_pump = wire_cleanup(&app, &notify);
    let net_refresh = wire_network(&app, &notify);
    let proc_refresh = wire_proc(&app, &notify);
    let _services_pump = wire_services(&app, &notify);
    let _events_pump = wire_events(&app);
    let _users_pump = wire_users(&app, &notify);
    let _posture_pump = wire_posture(&app, &notify);
    let _support_pump = wire_support(&app, &notify);
    let _printers_pump = wire_printers(&app, &notify);
    let _profiles_pump = wire_profiles(&app, &notify);
    let _disks_pump = wire_disks(&app);
    let _devices_pump = wire_devices(&app, &notify);
    let _certs_pump = wire_certs(&app);
    let _gpo_pump = wire_gpo(&app, &notify);
    wire_microwin(&app, &notify);
    wire_palette(&app);
    let power_refresh = wire_power(&app, &notify);
    let (_privacy_pump, privacy_refresh) = wire_privacy(
        &app,
        &jrnl,
        &journal_path,
        &notify,
        &tweaks_catalog,
        &tweaks_model,
    );
    let (_history_pump, history_refresh) = wire_history(
        &app,
        &jrnl,
        &journal_path,
        &notify,
        &tweaks_catalog,
        &tweaks_model,
    );
    let _config_pump = wire_config(
        &app,
        &jrnl,
        &journal_path,
        &notify,
        &tweaks_catalog,
        &tweaks_model,
        &modes_catalog,
    );
    wire_undo(
        &app,
        &jrnl,
        &journal_path,
        &notify,
        &tweaks_catalog,
        &tweaks_model,
        &modes_catalog,
    );
    apply_specs(&app);

    // Re-probe a panel's live state whenever the user navigates to it, so values
    // stay fresh across cross-panel changes (e.g. harden in Privacy → Tweaks).
    {
        let tcat = tweaks_catalog.clone();
        let tmodel = tweaks_model.clone();
        let net = net_refresh.clone();
        let proc = proc_refresh.clone();
        app.global::<Nav>().on_changed(move |page| match page {
            1 => refresh_tweaks(&tmodel, &tcat),
            3 => power_refresh(),
            6 => startup_refresh(),
            7 => features_refresh(),
            8 => privacy_refresh(),
            9 => history_refresh(),
            11 => net(),
            13 => proc(),
            _ => {}
        });
    }

    {
        let notify = notify.clone();
        let weak = app.as_weak();
        app.global::<Ui>().on_enable_sensors(move || {
            // With PawnIO present, go straight to the elevated sidecar. Without
            // it, CPU/board sensing can't work, so ask the user before installing
            // the driver rather than silently pulling it down.
            if sensors::pawnio_installed() {
                match sensors::spawn_elevated() {
                    Ok(()) => notify("info", "Requesting elevation for hardware sensors…"),
                    Err(e) => notify("error", &format!("Sensors failed: {e}")),
                }
            } else if let Some(app) = weak.upgrade() {
                app.global::<Ui>().set_pawnio_prompt(true);
            }
        });
    }

    {
        let notify = notify.clone();
        app.global::<Ui>().on_install_pawnio(move || {
            match launch_elevated_ps(&sensors::install_pawnio_script(), true) {
                Ok(()) => notify(
                    "info",
                    "Installing PawnIO, then starting sensors. Approve UAC.",
                ),
                Err(e) => notify("error", &format!("PawnIO install failed: {e}")),
            }
        });
    }

    let mut tele = Telemetry::new();
    apply_telemetry(&app, &tele.sample());

    // Rolling sparkline history for CPU + GPU load.
    let mut cpu_hist: std::collections::VecDeque<f32> = std::collections::VecDeque::new();
    let mut gpu_hist: std::collections::VecDeque<f32> = std::collections::VecDeque::new();

    let weak = app.as_weak();
    let timer = Timer::default();
    let mut tick = 0u64;
    timer.start(TimerMode::Repeated, Duration::from_secs(1), move || {
        if let Some(app) = weak.upgrade() {
            let s = tele.sample();
            apply_telemetry(&app, &s);
            spark_push(&mut cpu_hist, s.cpu_ratio);
            spark_push(&mut gpu_hist, s.gpu_ratio);
            let sys = app.global::<Sys>();
            sys.set_cpu_history(spark_model(&cpu_hist));
            sys.set_gpu_history(spark_model(&gpu_hist));
            // Live-refresh Network / Processes (every 2s) only while visible.
            tick += 1;
            if tick.is_multiple_of(2) {
                match app.global::<Nav>().get_page() {
                    11 => net_refresh(),
                    13 => proc_refresh(),
                    _ => {}
                }
            }
        }
    });

    app.run()
}
