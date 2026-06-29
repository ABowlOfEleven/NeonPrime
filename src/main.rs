// NeonPrime — a holographic system control deck for Windows.
//
// On Windows we don't want a console window tagging along with the GUI.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

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
    cleanup, config, debloat, dns, engine, features, installs, journal, modes, netmon, power,
    privacy, procmon, quick, repair, settings, startup, tweaks,
};

use telemetry::{Sample, Telemetry};

slint::include_modules!();

type SharedJournal = Rc<RefCell<Journal>>;
/// `notify(kind, message)` — kind is "success" | "error" | "info".
type Notify = Rc<dyn Fn(&str, &str)>;

/// Result of an off-thread elevated tweak, marshalled back to the UI thread.
/// Only `Send` data crosses the boundary (no `Rc`).
enum ElevatedMsg {
    Done { row_id: i32, name: String, want: bool, results: Vec<(Action, Reversal)> },
    Failed { row_id: i32, name: String, error: String },
}

/// Result of an elevated *revert* (History panel) coming back from the broker.
enum RevertMsg {
    Done { id: u64, label: String },
    Failed { label: String, error: String },
}

/// Background results for the Debloat panel.
enum DebloatMsg {
    Probed(std::collections::HashSet<String>),
    Removed { idx: i32, ok: bool, name: String, err: String },
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
    let ram = format!("{:.0} GiB", sp.total_memory() as f64 / (1024.0 * 1024.0 * 1024.0));

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
fn tweak_matches(row: &TweakRow, text: &str, cat: &str) -> bool {
    let cat_ok = cat == "ALL" || row.category.as_str() == cat;
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

fn run_local(actions: &[Action], jrnl: &SharedJournal, t: &tweaks::Tweak, want: bool) -> io::Result<()> {
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
                let _ = tx.send(ElevatedMsg::Failed { row_id, name, error: format!("elevation failed: {e}") });
                return;
            }
        }
    }
    let session = guard.as_mut().unwrap();
    let mut results = Vec::new();
    for a in &actions {
        match session.client.call(&Request::Apply { label: name.clone(), action: a.clone() }) {
            Ok(Response::Applied { reversal }) => results.push((a.clone(), reversal)),
            Ok(Response::Error(e)) => {
                let _ = tx.send(ElevatedMsg::Failed { row_id, name, error: e });
                return;
            }
            Ok(_) => {}
            Err(e) => {
                *guard = None; // drop a dead broker so the next attempt respawns it
                let _ = tx.send(ElevatedMsg::Failed { row_id, name, error: format!("broker link lost: {e}") });
                return;
            }
        }
    }
    let _ = tx.send(ElevatedMsg::Done { row_id, name, want, results });
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
    app.global::<Tweaks>().set_rows(ModelRc::from(filtered.clone()));
    {
        let weak = app.as_weak();
        let fs = filter_state.clone();
        let filtered = filtered.clone();
        app.global::<Tweaks>().on_filter(move || {
            if let Some(app) = weak.upgrade() {
                let t = app.global::<Tweaks>();
                *fs.borrow_mut() = (t.get_filter_text().to_lowercase(), t.get_filter_cat().to_string());
                filtered.reset();
            }
        });
    }

    // One-click "Essential Tweaks" — applies the curated no-elevation set.
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
            let Some(t) = cat.get(id as usize) else { return };

            if t.needs_elevation() {
                // Optimistic UI now; the privileged work happens off-thread so a
                // UAC prompt can't freeze the window. The pump corrects on failure.
                let mut r = make_row(id as usize, t);
                r.applied = want;
                model.set_row_data(id as usize, r);
                notify("info", "Requesting elevation — approve the UAC prompt…");

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
                    Ok(()) => notify("success", &format!("{} {}", t.name, if want { "applied" } else { "reverted" })),
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
                ElevatedMsg::Done { row_id, name, want, results } => {
                    {
                        let mut j = jrnl.borrow_mut();
                        for (a, rev) in results {
                            j.record(format!("{}: {}", name, if want { "on" } else { "off" }), a, rev);
                        }
                    }
                    let _ = jrnl.borrow().save(&path);
                    if let Some(t) = cat.get(row_id as usize) {
                        model.set_row_data(row_id as usize, make_row(row_id as usize, t));
                    }
                    notify("success", &format!("{} {}", name, if want { "applied" } else { "reverted" }));
                }
                ElevatedMsg::Failed { row_id, name, error } => {
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

fn wire_modes(app: &AppWindow, jrnl: &SharedJournal, journal_path: &Path, notify: &Notify, catalog: &Rc<Vec<modes::Mode>>) {
    let weak = app.as_weak();
    let cat = catalog.clone();
    let jrnl = jrnl.clone();
    let path = journal_path.to_path_buf();
    let notify = notify.clone();

    app.global::<Modes>().on_activate(move |idx| {
        let Some(m) = cat.get(idx as usize) else { return };

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
                    jrnl.borrow_mut().record(format!("mode: {}", m.name), a.clone(), reversal);
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

fn wire_installs(app: &AppWindow, notify: &Notify) {
    let catalog = Rc::new(installs::catalog());
    let rows: Vec<AppRow> = catalog
        .iter()
        .enumerate()
        .map(|(i, a)| AppRow {
            id: i as i32,
            name: a.name.as_str().into(),
            desc: a.desc.as_str().into(),
            category: a.category.as_str().into(),
        })
        .collect();
    let source = Rc::new(VecModel::from(rows));

    // Search filter over name / description / category (~300 apps).
    let filter_text = Rc::new(RefCell::new(String::new()));
    let filtered = Rc::new(FilterModel::new(ModelRc::from(source.clone()), {
        let ft = filter_text.clone();
        move |row: &AppRow| {
            let t = ft.borrow();
            t.is_empty()
                || row.name.to_lowercase().contains(t.as_str())
                || row.desc.to_lowercase().contains(t.as_str())
                || row.category.to_lowercase().contains(t.as_str())
        }
    }));
    app.global::<Installer>().set_rows(ModelRc::from(filtered.clone()));
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

    let cat = catalog.clone();
    let notify = notify.clone();
    let notify2 = notify.clone();
    app.global::<Installer>().on_install(move |id| {
        if let Some(a) = cat.get(id as usize) {
            match Command::new("winget").args(installs::install_args(&a.id)).spawn() {
                Ok(_) => notify("info", &format!("Installing {} via winget…", a.name)),
                Err(e) => notify("error", &format!("Couldn't start winget: {e}")),
            }
        }
    });

    {
        let notify = notify2.clone();
        app.global::<Installer>().on_update_all(move || {
            match launch_console("winget upgrade --all --include-unknown") {
                Ok(()) => notify("info", "Updating all apps — see the console window."),
                Err(e) => notify("error", &format!("winget: {e}")),
            }
        });
    }
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
) {
    let cfg_path = config::default_path();

    {
        let weak = app.as_weak();
        let cfg_path = cfg_path.clone();
        let notify = notify.clone();
        app.global::<Configuration>().on_export_config(move || {
            let cfg = config::capture();
            let toml = cfg.to_toml().unwrap_or_default();
            let _ = std::fs::write(&cfg_path, &toml);
            if let Some(app) = weak.upgrade() {
                let c = app.global::<Configuration>();
                c.set_preview(toml.as_str().into());
                c.set_status(format!("Exported → {}", cfg_path.display()).as_str().into());
            }
            notify(
                "success",
                &format!(
                    "Exported {} tweak(s), mode {}",
                    cfg.tweaks.len(),
                    cfg.mode.as_deref().unwrap_or("none")
                ),
            );
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
                app.global::<Configuration>().set_preview(toml.as_str().into());
                refresh_tweaks(&tmodel, &tcat);
                refresh_modes(&app, &mcat);
            }
            notify(
                "success",
                &format!("Applied {} tweak action(s), mode {}", applied, cfg.mode.as_deref().unwrap_or("none")),
            );
        });
    }

    // Fixes — elevated repair commands run in a visible console.
    {
        let notify = notify.clone();
        app.global::<Configuration>().on_run_fix(move |idx| {
            let Some((name, script)) = repair::fixes().get(idx as usize) else { return };
            match launch_elevated_ps(script, true) {
                Ok(()) => notify("info", &format!("{name} — approve UAC; progress shows in the console.")),
                Err(e) => notify("error", &format!("{name} failed: {e}")),
            }
        });
    }

    // Windows Update mode — elevated registry/service changes, run hidden.
    {
        let notify = notify.clone();
        app.global::<Configuration>().on_set_update_mode(move |idx| {
            let Some((name, script)) = repair::update_modes().get(idx as usize) else { return };
            match launch_elevated_ps(script, false) {
                Ok(()) => notify("success", &format!("Windows Update → {name} (approve UAC)")),
                Err(e) => notify("error", &format!("{name} failed: {e}")),
            }
        });
    }

    // Restore points — create one (elevated) or open the Windows wizard.
    {
        let notify = notify.clone();
        app.global::<Configuration>().on_create_restore_point(move || {
            let script = "Enable-ComputerRestore -Drive 'C:\\'; \
                Checkpoint-Computer -Description 'NeonPrime' -RestorePointType 'MODIFY_SETTINGS'; \
                Write-Host 'Restore point created.'";
            match launch_elevated_ps(script, true) {
                Ok(()) => notify("info", "Creating restore point — approve UAC; see the console."),
                Err(e) => notify("error", &format!("Restore point: {e}")),
            }
        });
    }
    {
        let notify = notify.clone();
        app.global::<Configuration>().on_open_system_restore(move || {
            match Command::new("rstrui.exe").spawn() {
                Ok(_) => notify("info", "Opening System Restore…"),
                Err(e) => notify("error", &format!("Couldn't open System Restore: {e}")),
            }
        });
    }
}

// ── Theme + Undo ────────────────────────────────────────────────────

fn wire_theme(app: &AppWindow) {
    let weak = app.as_weak();
    app.global::<Ui>().on_toggle_theme(move || {
        let Some(app) = weak.upgrade() else { return };
        let t = app.global::<Theme>();
        let new = !t.get_hev();
        t.set_hev(new);
        settings::Settings { theme_hev: new }.save();
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
        let entry = jrnl.borrow().entries.iter().rev().find(|e| e.active).cloned();
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
    let ps = format!("Start-Process -FilePath 'powershell' -ArgumentList {inner} -Verb RunAs{hidden}");
    Command::new("powershell")
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
    app.global::<Quick>().set_rows(Rc::new(VecModel::from(rows)).into());

    let cat = catalog.clone();
    let notify = notify.clone();
    app.global::<Quick>().on_run(move |id| {
        let Some(a) = cat.get(id as usize) else { return };

        // The PowerShell profile installer runs in a visible console (shows
        // winget/module install progress) from a script beside the app.
        if a.id == "install-ps-profile" {
            let mut script = std::env::current_exe().unwrap_or_default();
            script.pop();
            script.push("profile");
            script.push("install-profile.ps1");
            match Command::new("powershell")
                .args(["-NoExit", "-ExecutionPolicy", "Bypass", "-File", &script.to_string_lossy()])
                .spawn()
            {
                Ok(_) => notify("info", "Installing PowerShell profile — see the new window."),
                Err(e) => notify("error", &format!("Couldn't start installer: {e}")),
            }
            return;
        }

        let Some(inv) = quick::invocation(a.id) else { return };

        let result = if inv.elevated {
            // Launch elevated via UAC (Start-Process -Verb RunAs). Returns at once.
            let arglist = inv
                .args
                .iter()
                .map(|s| format!("'{}'", s.replace('\'', "''")))
                .collect::<Vec<_>>()
                .join(",");
            let ps = format!(
                "Start-Process -FilePath '{}' -ArgumentList {arglist} -Verb RunAs -WindowStyle Hidden",
                inv.program
            );
            Command::new("powershell")
                .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps])
                .spawn()
                .map(|_| ())
        } else {
            Command::new(&inv.program).args(&inv.args).spawn().map(|_| ())
        };

        match result {
            Ok(()) => notify("info", &format!("Running: {}", a.name)),
            Err(e) => notify("error", &format!("{} failed: {e}", a.name)),
        }
    });
}

fn wire_startup(app: &AppWindow, notify: &Notify) {
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
                Ok(()) => notify("success", &format!("{} {}", name, if want { "enabled" } else { "disabled" })),
                Err(e) => notify("error", &format!("{name}: {e}")),
            }
        }
        rebuild2();
    });
}

/// Windows optional features: enable/disable via elevated DISM in a visible
/// console. State isn't probed (DISM queries need elevation), so each row offers
/// explicit Enable/Disable like WinUtil.
fn wire_features(app: &AppWindow, notify: &Notify) {
    let rows: Vec<FeatureRow> = features::catalog()
        .iter()
        .enumerate()
        .map(|(i, f)| FeatureRow {
            id: i as i32,
            name: f.name.into(),
            desc: f.desc.into(),
        })
        .collect();
    app.global::<Features>().set_rows(Rc::new(VecModel::from(rows)).into());

    let notify = notify.clone();
    app.global::<Features>().on_apply(move |id, enable| {
        let Some(f) = features::catalog().get(id as usize) else { return };
        let script = features::dism_script(f, enable);
        let verb = if enable { "Enabling" } else { "Disabling" };
        match launch_elevated_ps(&script, true) {
            Ok(()) => notify(
                "info",
                &format!("{verb} {} — approve UAC; DISM progress shows in the console.", f.name),
            ),
            Err(e) => notify("error", &format!("{}: {e}", f.name)),
        }
    });
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
            let Some(b) = debloat::catalog().get(id as usize) else { return };
            notify("info", &format!("Removing {}…", b.name));
            let (tx, name) = (tx.clone(), b.name.to_string());
            std::thread::spawn(move || {
                let b = &debloat::catalog()[id as usize];
                let (ok, err) = match debloat::remove(b) {
                    Ok(o) => (o, String::new()),
                    Err(e) => (false, e.to_string()),
                };
                let _ = tx.send(DebloatMsg::Removed { idx: id, ok, name, err });
            });
        });
    }

    // Disable telemetry scheduled tasks (elevated, hidden console).
    {
        let notify = notify.clone();
        app.global::<Debloat>().on_disable_telemetry_tasks(move || {
            match launch_elevated_ps(&debloat::disable_tasks_script(), false) {
                Ok(()) => notify("info", "Disabling telemetry tasks — approve the UAC prompt…"),
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
                        notify2("error", &format!("{name}: removal blocked (system/provisioned app)"));
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
        let Some(script) = power::set_script(idx as usize) else { return };
        let name = power::plans().get(idx as usize).map(|p| p.name).unwrap_or("plan");
        match launch_elevated_ps(&script, false) {
            Ok(()) => notify("info", &format!("Switching to {name} — approve the UAC prompt…")),
            Err(e) => notify("error", &format!("Power plan failed: {e}")),
        }
    });
    refresh
}

/// Disk cleanup: scan reclaimable sizes off-thread, clean user targets in-process
/// and system caches via an elevated shell. Returns the result pump.
fn wire_cleanup(app: &AppWindow, notify: &Notify) -> Timer {
    let model: Rc<VecModel<CleanRow>> = Rc::new(VecModel::default());
    let rows: Vec<CleanRow> = cleanup::catalog()
        .iter()
        .enumerate()
        .map(|(i, t)| CleanRow {
            id: i as i32,
            name: t.name.into(),
            desc: t.desc.into(),
            size: "—".into(),
            elevated: t.elevated,
        })
        .collect();
    model.set_vec(rows);
    app.global::<Cleanup>().set_rows(model.clone().into());
    app.global::<Cleanup>().set_scanning(true);

    let sizes: Rc<RefCell<Vec<u64>>> = Rc::new(RefCell::new(vec![0; cleanup::catalog().len()]));
    let (tx, rx) = mpsc::channel::<CleanMsg>();

    let scan = {
        let tx = tx.clone();
        move || {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let v: Vec<u64> = cleanup::catalog().iter().map(|t| cleanup::size_of(t.id)).collect();
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

    // Clean one target.
    {
        let notify = notify.clone();
        let tx = tx.clone();
        app.global::<Cleanup>().on_clean(move |idx| {
            let Some(t) = cleanup::catalog().get(idx as usize) else { return };
            if t.elevated {
                if let Some(script) = cleanup::clean_script(t.id) {
                    match launch_elevated_ps(&script, false) {
                        Ok(()) => notify("info", &format!("Clearing {} — approve UAC, then RESCAN.", t.name)),
                        Err(e) => notify("error", &format!("{}: {e}", t.name)),
                    }
                }
            } else {
                notify("info", &format!("Cleaning {}…", t.name));
                let (tx, name) = (tx.clone(), t.name.to_string());
                std::thread::spawn(move || {
                    let id = cleanup::catalog()[idx as usize].id;
                    let _ = cleanup::clean(id);
                    let size = cleanup::size_of(id);
                    let _ = tx.send(CleanMsg::Cleaned { idx, size, name });
                });
            }
        });
    }

    // Pump.
    let weak = app.as_weak();
    let model2 = model.clone();
    let sizes2 = sizes.clone();
    let notify2 = notify.clone();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(200), move || {
        let mut dirty = false;
        while let Ok(msg) = rx.try_recv() {
            match msg {
                CleanMsg::Scanned(v) => {
                    for (i, &sz) in v.iter().enumerate() {
                        if let Some(mut row) = model2.row_data(i) {
                            row.size = cleanup::human(sz).into();
                            model2.set_row_data(i, row);
                        }
                    }
                    *sizes2.borrow_mut() = v;
                    if let Some(app) = weak.upgrade() {
                        app.global::<Cleanup>().set_scanning(false);
                    }
                    dirty = true;
                }
                CleanMsg::Cleaned { idx, size, name } => {
                    if let Some(mut row) = model2.row_data(idx as usize) {
                        row.size = cleanup::human(size).into();
                        model2.set_row_data(idx as usize, row);
                    }
                    if let Some(s) = sizes2.borrow_mut().get_mut(idx as usize) {
                        *s = size;
                    }
                    notify2("success", &format!("Cleaned: {name}"));
                    dirty = true;
                }
            }
        }
        if dirty {
            if let Some(app) = weak.upgrade() {
                let total: u64 = sizes2.borrow().iter().sum();
                app.global::<Cleanup>().set_total(cleanup::human(total).into());
            }
        }
    });
    timer
}

/// Process & resource monitor — top processes by CPU with per-process GPU/VRAM,
/// plus a kill action. Returns a refresh closure (nav + telemetry-tick driven).
fn wire_proc(app: &AppWindow, notify: &Notify) -> Rc<dyn Fn()> {
    let model: Rc<VecModel<ProcRow>> = Rc::new(VecModel::default());
    app.global::<Procs>().set_rows(model.clone().into());
    let monitor = Rc::new(RefCell::new(procmon::ProcMonitor::new()));

    let refresh: Rc<dyn Fn()> = {
        let weak = app.as_weak();
        let model = model.clone();
        let monitor = monitor.clone();
        Rc::new(move || {
            let procs = monitor.borrow_mut().snapshot(40);
            let rows: Vec<ProcRow> = procs
                .iter()
                .map(|p| {
                    let gpu = if p.gpu >= 0.5 { format!("{:.0}%", p.gpu) } else { "—".into() };
                    let vram = if p.vram > 0 { cleanup::human(p.vram) } else { "—".into() };
                    ProcRow {
                        pid: p.pid as i32,
                        name: p.name.as_str().into(),
                        cpu: format!("{:.0}%", p.cpu).as_str().into(),
                        mem: cleanup::human(p.mem).as_str().into(),
                        gpu: gpu.as_str().into(),
                        vram: vram.as_str().into(),
                    }
                })
                .collect();
            let n = rows.len() as i32;
            model.set_vec(rows);
            if let Some(app) = weak.upgrade() {
                app.global::<Procs>().set_count(n);
            }
        })
    };
    refresh();
    {
        let refresh = refresh.clone();
        app.global::<Procs>().on_refresh(move || refresh());
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

/// Network monitor — snapshot active outbound TCP connections per process.
/// Returns a refresh closure (driven by nav + the telemetry tick while visible).
fn wire_network(app: &AppWindow, notify: &Notify) -> Rc<dyn Fn()> {
    let model: Rc<VecModel<NetRow>> = Rc::new(VecModel::default());
    app.global::<Network>().set_rows(model.clone().into());

    let refresh: Rc<dyn Fn()> = {
        let weak = app.as_weak();
        let model = model.clone();
        Rc::new(move || {
            let rows: Vec<NetRow> = netmon::connections()
                .iter()
                .map(|c| NetRow {
                    proc_name: c.proc_name.as_str().into(),
                    pid: c.pid as i32,
                    remote: c.remote.as_str().into(),
                    state: c.state.as_str().into(),
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
    {
        let refresh = refresh.clone();
        app.global::<Network>().on_refresh(move || refresh());
    }
    {
        let notify = notify.clone();
        app.global::<Network>().on_set_dns(move |idx| {
            let Some(script) = dns::set_script(idx as usize) else { return };
            let name = dns::providers().get(idx as usize).map(|p| p.name).unwrap_or("DNS");
            match launch_elevated_ps(&script, false) {
                Ok(()) => notify("info", &format!("Setting DNS → {name} (approve UAC)")),
                Err(e) => notify("error", &format!("DNS: {e}")),
            }
        });
    }
    refresh
}

/// Command palette (Ctrl+K): fuzzy list of panels to jump to + actions to run.
/// id encodes the target — `<1000` nav page, `1000+` quick action, `2000+` mode.
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

    let model: Rc<VecModel<PaletteItem>> = Rc::new(VecModel::default());
    app.global::<Palette>().set_items(model.clone().into());

    {
        let weak = app.as_weak();
        let cmds = cmds.clone();
        let model = model.clone();
        app.global::<Palette>().on_filter(move || {
            let q = weak
                .upgrade()
                .map(|a| a.global::<Palette>().get_query().to_lowercase())
                .unwrap_or_default();
            let items: Vec<PaletteItem> = cmds
                .iter()
                .filter(|(l, _, _)| q.is_empty() || l.to_lowercase().contains(&q))
                .take(50)
                .map(|(l, h, id)| PaletteItem { label: l.as_str().into(), hint: (*h).into(), id: *id })
                .collect();
            model.set_vec(items);
        });
    }
    app.global::<Palette>().invoke_filter();

    {
        let weak = app.as_weak();
        app.global::<Palette>().on_run(move |id| {
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

/// Privacy/Hardening score — a view over the tweak catalog. Reads live state to
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
            let Some(t) = cat.get(id as usize) else { return };
            if t.needs_elevation() {
                notify("info", "Requesting elevation — approve the UAC prompt…");
                let (broker, tx, name, on) = (broker.clone(), tx.clone(), t.name.to_string(), t.on.clone());
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
                    let (broker, tx, name, on) = (broker.clone(), tx.clone(), t.name.to_string(), t.on.clone());
                    std::thread::spawn(move || elevated_worker(broker, tx, on, i as i32, name, true));
                } else if run_local(&t.on, &jrnl, t, true).is_ok() {
                    local += 1;
                }
            }
            let _ = jrnl.borrow().save(&path);
            refresh();
            if elevated > 0 {
                notify("info", &format!("Hardened {local} now; approve UAC for {elevated} more…"));
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
                ElevatedMsg::Done { name, want, results, .. } => {
                    {
                        let mut j = jrnl.borrow_mut();
                        for (a, rev) in results {
                            j.record(format!("{}: {}", name, if want { "on" } else { "off" }), a, rev);
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
                let _ = tx.send(RevertMsg::Failed { label, error: format!("elevation failed: {e}") });
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
            let _ = tx.send(RevertMsg::Failed { label, error: "unexpected broker reply".into() });
        }
        Err(e) => {
            *guard = None;
            let _ = tx.send(RevertMsg::Failed { label, error: format!("broker link lost: {e}") });
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
                notify("info", "Requesting elevation — approve the UAC prompt…");
                let (broker, tx, rev, label) =
                    (broker.clone(), tx.clone(), entry.reversal.clone(), entry.label.clone());
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
        app.global::<History>().on_revert(move |id| do_revert(id as u64));
    }

    // Revert every active entry, newest first.
    {
        let jrnl = jrnl.clone();
        let do_revert = do_revert.clone();
        app.global::<History>().on_revert_all(move || {
            let ids: Vec<u64> = jrnl.borrow().entries.iter().rev().filter(|e| e.active).map(|e| e.id).collect();
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

fn main() -> Result<(), slint::PlatformError> {
    // Single-instance guard — a second launch exits rather than racing the journal.
    let instance = single_instance::SingleInstance::new("neonprime-singleton").ok();
    if let Some(inst) = &instance {
        if !inst.is_single() {
            return Ok(());
        }
    }

    let app = AppWindow::new()?;
    let notify = make_notifier(&app);

    app.global::<Theme>().set_hev(settings::Settings::load().theme_hev);

    let journal_path: PathBuf = journal::default_path();
    let jrnl: SharedJournal = Rc::new(RefCell::new(Journal::load(&journal_path)));

    // Tweaks model.
    let tweaks_catalog = Rc::new(tweaks::catalog());
    let rows: Vec<TweakRow> = tweaks_catalog.iter().enumerate().map(|(i, t)| make_row(i, t)).collect();
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
    app.global::<Modes>().set_cards(Rc::new(VecModel::from(cards)).into());
    refresh_modes(&app, &modes_catalog);

    wire_theme(&app);
    let _tweak_pump = wire_tweaks(&app, &jrnl, &journal_path, &notify, &tweaks_catalog, &tweaks_model);
    wire_modes(&app, &jrnl, &journal_path, &notify, &modes_catalog);
    wire_installs(&app, &notify);
    wire_quick(&app, &notify);
    wire_startup(&app, &notify);
    wire_features(&app, &notify);
    let _debloat_pump = wire_debloat(&app, &notify);
    let _cleanup_pump = wire_cleanup(&app, &notify);
    let net_refresh = wire_network(&app, &notify);
    let proc_refresh = wire_proc(&app, &notify);
    wire_palette(&app);
    let power_refresh = wire_power(&app, &notify);
    let (_privacy_pump, privacy_refresh) =
        wire_privacy(&app, &jrnl, &journal_path, &notify, &tweaks_catalog, &tweaks_model);
    let (_history_pump, history_refresh) =
        wire_history(&app, &jrnl, &journal_path, &notify, &tweaks_catalog, &tweaks_model);
    wire_config(&app, &jrnl, &journal_path, &notify, &tweaks_catalog, &tweaks_model, &modes_catalog);
    wire_undo(&app, &jrnl, &journal_path, &notify, &tweaks_catalog, &tweaks_model, &modes_catalog);
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
            8 => privacy_refresh(),
            9 => history_refresh(),
            11 => net(),
            13 => proc(),
            _ => {}
        });
    }

    {
        let notify = notify.clone();
        app.global::<Ui>().on_enable_sensors(move || match sensors::spawn_elevated() {
            Ok(()) => notify("info", "Requesting elevation for hardware sensors…"),
            Err(e) => notify("error", &format!("Sensors failed: {e}")),
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
            if tick % 2 == 0 {
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
