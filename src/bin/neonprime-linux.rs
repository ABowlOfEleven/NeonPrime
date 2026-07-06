//! NeonPrime Linux UI binary.
//!
//! Brings up the Slint Linux window (`ui/linux.slint`) and wires it to the Linux
//! backend (`neonprime::core::linux`). Windows keeps its own binary; this one is
//! a no-op stub on non-Linux targets so the crate still builds everywhere.

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("neonprime-linux is the Linux UI binary; build it on Linux.");
}

#[cfg(target_os = "linux")]
fn main() -> Result<(), slint::PlatformError> {
    ui::run()
}

#[cfg(target_os = "linux")]
mod ui {
    slint::include_modules!();

    use std::cell::RefCell;
    use std::process::Command;
    use std::rc::Rc;
    use std::time::Duration;

    use slint::{Model, Timer, TimerMode, VecModel};

    use neonprime::core::linux::{
        autostart, cleanup, dns, firewall, netmon, pkg, power, procmon, quick, services, telemetry,
        ElevatedCmd,
    };

    const HISTORY: usize = 60;

    fn fmt_uptime(secs: u64) -> String {
        let (d, h, m) = (secs / 86400, (secs % 86400) / 3600, (secs % 3600) / 60);
        if d > 0 {
            format!("{d}d {h}h {m}m")
        } else if h > 0 {
            format!("{h}h {m}m")
        } else {
            format!("{m}m")
        }
    }

    fn gib(bytes: u64) -> f64 {
        bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    }

    /// Run a command line detached; returns a short status string.
    fn spawn_cmd(argv: &[String]) -> String {
        if argv.is_empty() {
            return "nothing to run".into();
        }
        match Command::new(&argv[0]).args(&argv[1..]).spawn() {
            Ok(_) => format!("running: {}", argv.join(" ")),
            Err(e) => format!("failed: {e}"),
        }
    }

    /// Run an ElevatedCmd, wrapping in pkexec when it needs privilege.
    fn run_elevated(cmd: &ElevatedCmd, privileged: bool) -> String {
        let argv = if privileged {
            cmd.pkexec()
        } else {
            cmd.argv.clone()
        };
        spawn_cmd(&argv)
    }

    pub fn run() -> Result<(), slint::PlatformError> {
        let app = AppWindow::new()?;

        wire_specs(&app);
        let tele = wire_telemetry(&app);
        let refresh_procs = wire_procs(&app);
        let refresh_net = wire_network(&app);
        wire_services(&app);
        wire_packages(&app);
        wire_dns(&app);
        wire_cleanup(&app);
        wire_power(&app);
        wire_quick(&app);
        let refresh_fw = wire_firewall(&app);
        let refresh_auto = wire_autostart(&app);

        // Nav change: refresh the panel being shown.
        {
            let weak = app.as_weak();
            let refresh_procs = refresh_procs.clone();
            let refresh_net = refresh_net.clone();
            app.global::<Nav>().on_changed(move |page| {
                let Some(app) = weak.upgrade() else { return };
                match page {
                    1 => refresh_procs(),
                    2 => refresh_net(),
                    3 => app.global::<Services>().invoke_refresh(),
                    9 => refresh_fw(),
                    10 => refresh_auto(),
                    _ => {}
                }
            });
        }

        // 1 Hz telemetry tick; also drives the live monitor panels.
        let timer = Timer::default();
        {
            let weak = app.as_weak();
            let tele = tele.clone();
            let refresh_procs = refresh_procs.clone();
            let refresh_net = refresh_net.clone();
            timer.start(
                TimerMode::Repeated,
                Duration::from_millis(1000),
                move || {
                    let Some(app) = weak.upgrade() else { return };
                    tele();
                    match app.global::<Nav>().get_page() {
                        1 => refresh_procs(),
                        2 => refresh_net(),
                        _ => {}
                    }
                },
            );
        }

        app.run()
    }

    /// Static system specs, gathered once.
    fn wire_specs(app: &AppWindow) {
        use sysinfo::System;
        let mut sys = System::new_all();
        sys.refresh_cpu_usage();
        let os = System::long_os_version().unwrap_or_else(|| "Linux".into());
        let kernel = System::kernel_version().unwrap_or_default();
        let cpu = sys
            .cpus()
            .first()
            .map(|c| c.brand().trim().to_string())
            .unwrap_or_default();
        let ram = format!("{:.0} GiB", gib(sys.total_memory()).round());
        let s = app.global::<Sys>();
        s.set_spec_os(os.into());
        s.set_spec_kernel(kernel.into());
        s.set_spec_cpu(cpu.into());
        s.set_spec_ram(ram.into());
    }

    /// Live CPU/RAM/temp/load, plus the rolling CPU history sparkline.
    fn wire_telemetry(app: &AppWindow) -> Rc<dyn Fn()> {
        let mon = Rc::new(RefCell::new(telemetry::Telemetry::new()));
        let history: Rc<VecModel<f32>> = Rc::new(VecModel::default());
        app.global::<Sys>().set_cpu_history(history.clone().into());

        let weak = app.as_weak();
        Rc::new(move || {
            let Some(app) = weak.upgrade() else { return };
            let sample = mon.borrow_mut().sample();
            let s = app.global::<Sys>();

            let cpu_ratio = (sample.cpu / 100.0).clamp(0.0, 1.0);
            s.set_cpu_ratio(cpu_ratio);
            s.set_cpu_text(format!("{:.0}%", sample.cpu).into());

            if sample.mem_total > 0 {
                s.set_ram_ratio((sample.mem_used as f64 / sample.mem_total as f64) as f32);
                s.set_ram_text(
                    format!(
                        "{:.1} / {:.0} G",
                        gib(sample.mem_used),
                        gib(sample.mem_total)
                    )
                    .into(),
                );
            }

            match sample.cpu_temp {
                Some(t) => {
                    s.set_temp_ratio((t / 100.0).clamp(0.0, 1.0));
                    s.set_temp_text(format!("{t:.0}°C").into());
                    s.set_temp_warn(t >= 85.0);
                }
                None => {
                    s.set_temp_text("N/A".into());
                    s.set_temp_ratio(0.0);
                }
            }

            s.set_load_text(format!("load {:.2}", sample.load1).into());
            s.set_spec_uptime(fmt_uptime(sysinfo::System::uptime()).into());

            if history.row_count() >= HISTORY {
                history.remove(0);
            }
            history.push(cpu_ratio);
        })
    }

    /// Process manager: last snapshot retained so sort/filter re-render cheaply.
    fn wire_procs(app: &AppWindow) -> Rc<dyn Fn()> {
        let model: Rc<VecModel<ProcRow>> = Rc::new(VecModel::default());
        app.global::<Procs>().set_rows(model.clone().into());
        let mon = Rc::new(RefCell::new(procmon::ProcMonitor::new()));
        let last: Rc<RefCell<Vec<procmon::Proc>>> = Rc::new(RefCell::new(Vec::new()));

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
                match key {
                    1 => procs.sort_by_key(|p| std::cmp::Reverse(p.mem)),
                    2 => procs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
                    _ => procs.sort_by(|a, b| {
                        b.cpu
                            .partial_cmp(&a.cpu)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    }),
                }
                procs.truncate(60);
                let rows: Vec<ProcRow> = procs
                    .iter()
                    .map(|p| ProcRow {
                        pid: p.pid as i32,
                        name: p.name.as_str().into(),
                        cpu: format!("{:.0}%", p.cpu).into(),
                        mem: neonprime::core::linux::human(p.mem).into(),
                    })
                    .collect();
                let n = rows.len() as i32;
                model.set_vec(rows);
                app.global::<Procs>().set_count(n);
            })
        };

        let refresh: Rc<dyn Fn()> = {
            let mon = mon.clone();
            let last = last.clone();
            let apply = apply.clone();
            Rc::new(move || {
                *last.borrow_mut() = mon.borrow_mut().snapshot(300);
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
            let refresh = refresh.clone();
            app.global::<Procs>().on_kill(move |pid| {
                procmon::terminate(pid as u32);
                refresh();
            });
        }
        refresh
    }

    /// Network monitor with cached reverse-DNS.
    fn wire_network(app: &AppWindow) -> Rc<dyn Fn()> {
        let model: Rc<VecModel<NetRow>> = Rc::new(VecModel::default());
        app.global::<Network>().set_rows(model.clone().into());
        let resolver = netmon::Resolver::new();

        let refresh: Rc<dyn Fn()> = {
            let weak = app.as_weak();
            let model = model.clone();
            Rc::new(move || {
                let Some(app) = weak.upgrade() else { return };
                let rows: Vec<NetRow> = netmon::connections()
                    .iter()
                    .map(|c| NetRow {
                        proc_name: c.proc_name.as_str().into(),
                        pid: c.pid as i32,
                        remote: c.remote.as_str().into(),
                        host: resolver.host(c.remote_ip).into(),
                        state: c.state.as_str().into(),
                    })
                    .collect();
                let n = rows.len() as i32;
                model.set_vec(rows);
                app.global::<Network>().set_count(n);
            })
        };
        refresh();
        {
            let refresh = refresh.clone();
            app.global::<Network>().on_refresh(move || refresh());
        }
        refresh
    }

    /// systemd services: loaded lazily, filtered from a retained list.
    fn wire_services(app: &AppWindow) {
        let model: Rc<VecModel<SvcRow>> = Rc::new(VecModel::default());
        app.global::<Services>().set_rows(model.clone().into());
        let full: Rc<RefCell<Vec<services::Svc>>> = Rc::new(RefCell::new(Vec::new()));

        let apply: Rc<dyn Fn()> = {
            let weak = app.as_weak();
            let model = model.clone();
            let full = full.clone();
            Rc::new(move || {
                let Some(app) = weak.upgrade() else { return };
                let q = app.global::<Services>().get_filter_text().to_lowercase();
                let rows: Vec<SvcRow> = full
                    .borrow()
                    .iter()
                    .filter(|s| {
                        q.is_empty()
                            || s.name.to_lowercase().contains(&q)
                            || s.description.to_lowercase().contains(&q)
                    })
                    .map(|s| SvcRow {
                        name: s.name.as_str().into(),
                        description: s.description.as_str().into(),
                        running: s.running,
                        enabled: s.enabled,
                    })
                    .collect();
                model.set_vec(rows);
            })
        };

        {
            let weak = app.as_weak();
            let full = full.clone();
            let apply = apply.clone();
            app.global::<Services>().on_refresh(move || {
                if let Some(app) = weak.upgrade() {
                    app.global::<Services>().set_scanning(true);
                }
                *full.borrow_mut() = services::services();
                if let Some(app) = weak.upgrade() {
                    app.global::<Services>().set_scanning(false);
                }
                apply();
            });
        }
        {
            let apply = apply.clone();
            app.global::<Services>().on_filter(move || apply());
        }
        app.global::<Services>()
            .on_start(|name| _ = run_elevated(&services::start(&name), true));
        app.global::<Services>()
            .on_stop(|name| _ = run_elevated(&services::stop(&name), true));
        app.global::<Services>()
            .on_enable(|name| _ = run_elevated(&services::enable(&name), true));
        app.global::<Services>()
            .on_disable(|name| _ = run_elevated(&services::disable(&name), true));
    }

    /// Package managers: detect, then install/remove/update via the selected one.
    fn wire_packages(app: &AppWindow) {
        let managers = pkg::detected();
        let items: Vec<PkgMgrItem> = managers
            .iter()
            .enumerate()
            .map(|(i, m)| PkgMgrItem {
                id: i as i32,
                label: m.label().into(),
            })
            .collect();
        app.global::<Packages>()
            .set_managers(Rc::new(VecModel::from(items)).into());
        let managers = Rc::new(managers);

        let selected = {
            let weak = app.as_weak();
            let managers = managers.clone();
            move || -> Option<pkg::Manager> {
                let idx = weak.upgrade()?.global::<Packages>().get_selected() as usize;
                managers.get(idx).copied()
            }
        };

        {
            let weak = app.as_weak();
            let selected = selected.clone();
            app.global::<Packages>().on_install(move |name| {
                let Some(m) = selected() else { return };
                if name.is_empty() {
                    return;
                }
                let status = run_elevated(&pkg::install(m, &name), pkg::needs_privilege(m));
                if let Some(app) = weak.upgrade() {
                    app.global::<Packages>().set_status(status.into());
                }
            });
        }
        {
            let weak = app.as_weak();
            let selected = selected.clone();
            app.global::<Packages>().on_remove(move |name| {
                let Some(m) = selected() else { return };
                if name.is_empty() {
                    return;
                }
                let status = run_elevated(&pkg::remove(m, &name), pkg::needs_privilege(m));
                if let Some(app) = weak.upgrade() {
                    app.global::<Packages>().set_status(status.into());
                }
            });
        }
        {
            let weak = app.as_weak();
            let selected = selected.clone();
            app.global::<Packages>().on_update_all(move || {
                let Some(m) = selected() else { return };
                let status = run_elevated(&pkg::update_all(m), pkg::needs_privilege(m));
                if let Some(app) = weak.upgrade() {
                    app.global::<Packages>().set_status(status.into());
                }
            });
        }
    }

    /// DNS switcher over the default-route link.
    fn wire_dns(app: &AppWindow) {
        let items: Vec<DnsProv> = dns::providers()
            .iter()
            .enumerate()
            .map(|(i, p)| DnsProv {
                id: i as i32,
                name: p.name.into(),
            })
            .collect();
        app.global::<Dns>()
            .set_providers(Rc::new(VecModel::from(items)).into());
        let link = dns::default_link().unwrap_or_else(|| "none".into());
        app.global::<Dns>().set_link(link.as_str().into());

        app.global::<Dns>().on_set(move |idx| {
            if let Some(cmd) = dns::set_cmd(idx as usize, &link) {
                let _ = run_elevated(&cmd, true);
            }
        });
    }

    /// Build the Cleanup model from a fresh set of per-target sizes: largest
    /// first, each with its share of the biggest for the bar. Runs on the UI
    /// thread (posted from the scan thread).
    fn fill_cleanup(app: &AppWindow, sizes: &[u64]) {
        let max = sizes.iter().copied().max().unwrap_or(0).max(1);
        let mut indexed: Vec<(usize, CleanRow)> = cleanup::catalog()
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let sz = sizes.get(i).copied().unwrap_or(0);
                (
                    i,
                    CleanRow {
                        id: i as i32,
                        name: t.name.into(),
                        desc: t.desc.into(),
                        size: neonprime::core::linux::human(sz).into(),
                        frac: sz as f32 / max as f32,
                        elevated: t.elevated,
                    },
                )
            })
            .collect();
        indexed.sort_by(|a, b| {
            sizes
                .get(b.0)
                .copied()
                .unwrap_or(0)
                .cmp(&sizes.get(a.0).copied().unwrap_or(0))
        });
        let rows: Vec<CleanRow> = indexed.into_iter().map(|(_, r)| r).collect();
        let total: u64 = sizes.iter().sum();
        app.global::<Cleanup>()
            .set_rows(Rc::new(VecModel::from(rows)).into());
        app.global::<Cleanup>()
            .set_total(neonprime::core::linux::human(total).into());
        app.global::<Cleanup>().set_scanning(false);
    }

    /// Disk cleanup: sizes scanned off-thread, cleaned in-process (user) or via
    /// pkexec (system).
    fn wire_cleanup(app: &AppWindow) {
        app.global::<Cleanup>().set_scanning(true);

        let scan = {
            let weak = app.as_weak();
            move || {
                let weak = weak.clone();
                std::thread::spawn(move || {
                    let sizes: Vec<u64> = cleanup::catalog()
                        .iter()
                        .map(|t| cleanup::size_of(t.id))
                        .collect();
                    let _ = weak.upgrade_in_event_loop(move |app| fill_cleanup(&app, &sizes));
                });
            }
        };
        scan();

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
        {
            let weak = app.as_weak();
            app.global::<Cleanup>().on_clean(move |idx| {
                let Some(t) = cleanup::catalog().get(idx as usize) else {
                    return;
                };
                if t.elevated {
                    if let Some(cmd) = cleanup::clean_cmd(t.id) {
                        let _ = run_elevated(&cmd, true);
                    }
                    return;
                }
                let id = t.id;
                let weak = weak.clone();
                std::thread::spawn(move || {
                    let _ = cleanup::clean(id);
                    let sizes: Vec<u64> = cleanup::catalog()
                        .iter()
                        .map(|t| cleanup::size_of(t.id))
                        .collect();
                    let _ = weak.upgrade_in_event_loop(move |app| fill_cleanup(&app, &sizes));
                });
            });
        }
    }

    /// Power profiles via power-profiles-daemon.
    fn wire_power(app: &AppWindow) {
        app.global::<Power>().set_available(power::available());
        let profs: Vec<PowerProfile> = power::profiles()
            .iter()
            .enumerate()
            .map(|(i, p)| PowerProfile {
                id: i as i32,
                name: p.name.into(),
            })
            .collect();
        app.global::<Power>()
            .set_profiles(Rc::new(VecModel::from(profs)).into());

        let set_active = {
            let weak = app.as_weak();
            move || {
                let Some(app) = weak.upgrade() else { return };
                let idx = power::active()
                    .and_then(|a| power::profiles().iter().position(|p| p.id == a.as_str()))
                    .map(|i| i as i32)
                    .unwrap_or(-1);
                app.global::<Power>().set_active(idx);
            }
        };
        set_active();

        {
            let set_active = set_active.clone();
            app.global::<Power>().on_set(move |idx| {
                if let Some(p) = power::profiles().get(idx as usize) {
                    let argv = power::set_argv(p.id);
                    let _ = Command::new(&argv[0]).args(&argv[1..]).status();
                }
                set_active();
            });
        }
    }

    /// Quick maintenance actions.
    fn wire_quick(app: &AppWindow) {
        let items: Vec<QuickItem> = quick::catalog()
            .iter()
            .enumerate()
            .map(|(i, a)| QuickItem {
                id: i as i32,
                name: a.name.into(),
                desc: a.desc.into(),
                privileged: a.privileged,
            })
            .collect();
        app.global::<Quick>()
            .set_actions(Rc::new(VecModel::from(items)).into());

        let weak = app.as_weak();
        app.global::<Quick>().on_run(move |idx| {
            let Some(a) = quick::catalog().get(idx as usize) else {
                return;
            };
            let Some(argv) = quick::run_argv(a.id) else {
                return;
            };
            let argv = if a.privileged {
                let mut v = vec!["pkexec".to_string()];
                v.extend(argv);
                v
            } else {
                argv
            };
            let status = spawn_cmd(&argv);
            if let Some(app) = weak.upgrade() {
                app.global::<Quick>().set_status(status.into());
            }
        });
    }

    /// ufw firewall: enabled state (read from config) + enable/disable/reset.
    fn wire_firewall(app: &AppWindow) -> Rc<dyn Fn()> {
        let refresh: Rc<dyn Fn()> = {
            let weak = app.as_weak();
            Rc::new(move || {
                let Some(app) = weak.upgrade() else { return };
                let fw = app.global::<Firewall>();
                fw.set_available(firewall::available());
                fw.set_enabled(match firewall::enabled() {
                    Some(true) => 1,
                    Some(false) => 0,
                    None => -1,
                });
            })
        };
        refresh();

        let bind = |cmd: ElevatedCmd, refresh: Rc<dyn Fn()>| {
            move || {
                let _ = run_elevated(&cmd, true);
                refresh();
            }
        };
        app.global::<Firewall>()
            .on_enable(bind(firewall::enable(), refresh.clone()));
        app.global::<Firewall>()
            .on_disable(bind(firewall::disable(), refresh.clone()));
        app.global::<Firewall>()
            .on_reset(bind(firewall::reset(), refresh.clone()));
        refresh
    }

    /// XDG autostart entries + enable/disable via the Hidden key (no elevation).
    fn wire_autostart(app: &AppWindow) -> Rc<dyn Fn()> {
        let refresh: Rc<dyn Fn()> = {
            let weak = app.as_weak();
            Rc::new(move || {
                let Some(app) = weak.upgrade() else { return };
                let rows: Vec<AutostartRow> = autostart::entries()
                    .iter()
                    .map(|e| AutostartRow {
                        name: e.name.as_str().into(),
                        file: e.file.as_str().into(),
                        enabled: e.enabled,
                        system: e.system,
                    })
                    .collect();
                app.global::<Autostart>()
                    .set_rows(Rc::new(VecModel::from(rows)).into());
            })
        };
        refresh();
        {
            let refresh = refresh.clone();
            app.global::<Autostart>().on_refresh(move || refresh());
        }
        {
            let refresh = refresh.clone();
            app.global::<Autostart>().on_toggle(move |file, enabled| {
                let _ = autostart::set_enabled(&file, enabled);
                refresh();
            });
        }
        refresh
    }
}
