// NeonPrime — a holographic system control deck for Windows.
//
// On Windows we don't want a console window tagging along with the GUI.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod telemetry;

use std::cell::RefCell;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use slint::{Model, Timer, TimerMode, VecModel};

use neonprime::core::action::Action;
use neonprime::core::ipc::{Request, Response};
use neonprime::core::journal::Journal;
use neonprime::core::session::BrokerSession;
use neonprime::core::{engine, installs, journal, modes, tweaks};

use telemetry::{Sample, Telemetry};

slint::include_modules!();

type SharedJournal = Rc<RefCell<Journal>>;

/// Copy a telemetry sample into the UI's `Sys` global.
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

/// Apply/revert an HKCU (unelevated) tweak directly, journaling each change.
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

/// Apply/revert an HKLM (elevated) tweak through the broker, spawning it (UAC)
/// on first use.
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

fn wire_tweaks(app: &AppWindow, jrnl: &SharedJournal, journal_path: &Path) {
    let catalog = Rc::new(tweaks::catalog());
    let rows: Vec<TweakRow> = catalog.iter().enumerate().map(|(i, t)| make_row(i, t)).collect();
    let model = Rc::new(VecModel::from(rows));
    app.global::<Tweaks>().set_rows(model.clone().into());

    let broker: Rc<RefCell<Option<BrokerSession>>> = Rc::new(RefCell::new(None));
    let cat = catalog.clone();
    let jrnl = jrnl.clone();
    let path = journal_path.to_path_buf();

    app.global::<Tweaks>().on_toggle(move |id, want| {
        let Some(t) = cat.get(id as usize) else { return };
        let actions = if want { &t.on } else { &t.off };

        let result = if t.needs_elevation() {
            run_elevated(&broker, &jrnl, actions, t)
        } else {
            run_local(actions, &jrnl, t, want)
        };
        if let Err(e) = result {
            eprintln!("tweak '{}' toggle failed: {e}", t.id);
        }
        let _ = jrnl.borrow().save(&path);
        model.set_row_data(id as usize, make_row(id as usize, t));
    });
}

// ── Modes ───────────────────────────────────────────────────────────

fn wire_modes(app: &AppWindow, jrnl: &SharedJournal, journal_path: &Path) {
    let catalog = Rc::new(modes::catalog());
    let cards: Vec<ModeCard> = catalog
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

    let active_idx = modes::active()
        .and_then(|id| catalog.iter().position(|m| m.id == id))
        .map(|i| i as i32)
        .unwrap_or(-1);
    app.global::<Modes>().set_active(active_idx);

    let weak = app.as_weak();
    let cat = catalog.clone();
    let jrnl = jrnl.clone();
    let path = journal_path.to_path_buf();

    app.global::<Modes>().on_activate(move |idx| {
        let Some(m) = cat.get(idx as usize) else { return };
        for a in &m.actions {
            match engine::apply(a) {
                Ok(reversal) => {
                    jrnl.borrow_mut().record(format!("mode: {}", m.name), a.clone(), reversal);
                }
                Err(e) => eprintln!("mode '{}' action failed: {e}", m.id),
            }
        }
        let _ = jrnl.borrow().save(&path);
        if let Some(app) = weak.upgrade() {
            app.global::<Modes>().set_active(idx);
        }
    });
}

// ── Installs ────────────────────────────────────────────────────────

fn wire_installs(app: &AppWindow) {
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
    app.global::<Installer>().on_install(move |id| {
        if let Some(a) = cat.get(id as usize) {
            let _ = std::process::Command::new("winget")
                .args(installs::install_args(a.id))
                .spawn();
        }
    });
}

fn main() -> Result<(), slint::PlatformError> {
    let app = AppWindow::new()?;

    let journal_path: PathBuf = journal::default_path();
    let jrnl: SharedJournal = Rc::new(RefCell::new(Journal::load(&journal_path)));

    wire_tweaks(&app, &jrnl, &journal_path);
    wire_modes(&app, &jrnl, &journal_path);
    wire_installs(&app);

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
