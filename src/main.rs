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
use neonprime::core::{config, engine, installs, journal, modes, quick, settings, startup, tweaks};

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

fn wire_modes(app: &AppWindow, jrnl: &SharedJournal, journal_path: &Path, notify: &Notify, catalog: &Rc<Vec<modes::Mode>>) {
    let weak = app.as_weak();
    let cat = catalog.clone();
    let jrnl = jrnl.clone();
    let path = journal_path.to_path_buf();
    let notify = notify.clone();

    app.global::<Modes>().on_activate(move |idx| {
        let Some(m) = cat.get(idx as usize) else { return };
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
    app.global::<Installer>().on_install(move |id| {
        if let Some(a) = cat.get(id as usize) {
            match Command::new("winget").args(installs::install_args(&a.id)).spawn() {
                Ok(_) => notify("info", &format!("Installing {} via winget…", a.name)),
                Err(e) => notify("error", &format!("Couldn't start winget: {e}")),
            }
        }
    });
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
    wire_config(&app, &jrnl, &journal_path, &notify, &tweaks_catalog, &tweaks_model, &modes_catalog);
    wire_undo(&app, &jrnl, &journal_path, &notify, &tweaks_catalog, &tweaks_model, &modes_catalog);
    apply_specs(&app);

    {
        let notify = notify.clone();
        app.global::<Ui>().on_enable_sensors(move || match sensors::spawn_elevated() {
            Ok(()) => notify("info", "Requesting elevation for hardware sensors…"),
            Err(e) => notify("error", &format!("Sensors failed: {e}")),
        });
    }

    let mut tele = Telemetry::new();
    apply_telemetry(&app, &tele.sample());

    let weak = app.as_weak();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_secs(1), move || {
        if let Some(app) = weak.upgrade() {
            apply_telemetry(&app, &tele.sample());
        }
    });

    app.run()
}
