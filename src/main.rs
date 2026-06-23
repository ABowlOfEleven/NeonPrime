// NeonPrime — a holographic system control deck for Windows.
//
// On Windows we don't want a console window tagging along with the GUI.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod telemetry;

use std::cell::{Cell, RefCell};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::time::Duration;

use slint::{Model, Timer, TimerMode, VecModel};

use neonprime::core::action::Action;
use neonprime::core::ipc::{Request, Response};
use neonprime::core::journal::Journal;
use neonprime::core::session::BrokerSession;
use neonprime::core::{config, engine, installs, journal, modes, settings, tweaks};

use telemetry::{Sample, Telemetry};

slint::include_modules!();

type SharedJournal = Rc<RefCell<Journal>>;
/// `notify(kind, message)` — kind is "success" | "error" | "info".
type Notify = Rc<dyn Fn(&str, &str)>;

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

fn run_elevated(
    broker: &Rc<RefCell<Option<BrokerSession>>>,
    jrnl: &SharedJournal,
    actions: &[Action],
    t: &tweaks::Tweak,
) -> io::Result<()> {
    if broker.borrow().is_none() {
        *broker.borrow_mut() = Some(BrokerSession::spawn(true)?); // triggers UAC
    }
    let mut guard = broker.borrow_mut();
    let session = guard.as_mut().unwrap();
    for a in actions {
        match session
            .client
            .call(&Request::Apply { label: t.name.into(), action: a.clone() })?
        {
            Response::Applied { reversal } => {
                jrnl.borrow_mut().record(t.name.to_string(), a.clone(), reversal);
            }
            Response::Error(e) => return Err(io::Error::other(e)),
            _ => {}
        }
    }
    Ok(())
}

fn wire_tweaks(
    app: &AppWindow,
    jrnl: &SharedJournal,
    journal_path: &Path,
    notify: &Notify,
    catalog: &Rc<Vec<tweaks::Tweak>>,
    model: &Rc<VecModel<TweakRow>>,
) {
    let broker: Rc<RefCell<Option<BrokerSession>>> = Rc::new(RefCell::new(None));
    let cat = catalog.clone();
    let model = model.clone();
    let jrnl = jrnl.clone();
    let path = journal_path.to_path_buf();
    let notify = notify.clone();

    app.global::<Tweaks>().on_toggle(move |id, want| {
        let Some(t) = cat.get(id as usize) else { return };
        let actions = if want { &t.on } else { &t.off };

        let result = if t.needs_elevation() {
            run_elevated(&broker, &jrnl, actions, t)
        } else {
            run_local(actions, &jrnl, t, want)
        };
        match &result {
            Ok(()) => notify("success", &format!("{} {}", t.name, if want { "applied" } else { "reverted" })),
            Err(e) => notify("error", &format!("{}: {}", t.name, e)),
        }
        let _ = jrnl.borrow().save(&path);
        // Re-probe: on failure the row snaps back to reality (no lying toggle).
        model.set_row_data(id as usize, make_row(id as usize, t));
    });
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
            name: a.name.into(),
            publisher: a.publisher.into(),
            category: a.category.into(),
        })
        .collect();
    app.global::<Installer>().set_rows(Rc::new(VecModel::from(rows)).into());

    let cat = catalog.clone();
    let notify = notify.clone();
    app.global::<Installer>().on_install(move |id| {
        if let Some(a) = cat.get(id as usize) {
            match Command::new("winget").args(installs::install_args(a.id)).spawn() {
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
    app.global::<Tweaks>().set_rows(tweaks_model.clone().into());

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
    wire_tweaks(&app, &jrnl, &journal_path, &notify, &tweaks_catalog, &tweaks_model);
    wire_modes(&app, &jrnl, &journal_path, &notify, &modes_catalog);
    wire_installs(&app, &notify);
    wire_config(&app, &jrnl, &journal_path, &notify, &tweaks_catalog, &tweaks_model, &modes_catalog);
    wire_undo(&app, &jrnl, &journal_path, &notify, &tweaks_catalog, &tweaks_model, &modes_catalog);

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
