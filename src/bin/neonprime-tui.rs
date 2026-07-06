//! NeonPrime terminal UI (`neonprime-tui`).
//!
//! A headless/SSH-friendly ratatui front end over the same Linux backend
//! (`neonprime::core::linux`) the desktop UI uses. No display or GUI libraries
//! are needed at runtime. Privileged actions suspend the TUI and run through
//! `sudo` so the password prompt works in the terminal (unlike the GUI, which
//! uses a graphical `pkexec`). On non-Linux targets this is an inert stub.

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("neonprime-tui is the Linux terminal UI; build it on Linux.");
}

#[cfg(target_os = "linux")]
fn main() -> std::io::Result<()> {
    tui::run()
}

#[cfg(target_os = "linux")]
mod tui {
    use std::io::{self, Write};
    use std::process::Command;

    use crossterm::event::{self, Event, KeyCode, KeyEventKind};
    use ratatui::layout::{Constraint, Direction, Layout};
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
    use ratatui::{DefaultTerminal, Frame};

    use neonprime::core::linux::{
        apps, autostart, cleanup, debloat, dns, firewall, gaming, gpudriver, pkg, power, quick,
        restore, servers, services, telemetry, tweaks,
    };

    const CATS: &[&str] = &[
        "Dashboard",
        "Tweaks",
        "Quick Actions",
        "Cleanup",
        "Debloat",
        "Power",
        "DNS",
        "Firewall",
        "Autostart",
        "Restore",
        "Packages",
        "Services",
        "Graphics",
        "Servers",
    ];

    /// What activating an item does.
    enum Outcome {
        /// Already handled in-process; show this status.
        Inline(String),
        /// Suspend the TUI and run this command (prefixed with sudo if privileged).
        Shell(Vec<String>, bool),
    }

    struct Item {
        label: String,
        detail: String,
        run: Box<dyn Fn() -> Outcome>,
    }

    fn info(label: impl Into<String>) -> Item {
        Item {
            label: label.into(),
            detail: String::new(),
            run: Box::new(|| Outcome::Inline(String::new())),
        }
    }

    /// Build the item list for a category, reading live state each time.
    fn build_items(cat: usize, mgr: Option<pkg::Manager>, link: &str) -> Vec<Item> {
        match cat {
            // 0 Dashboard is rendered specially (no items).
            1 => tweaks_items(),
            2 => quick::catalog()
                .iter()
                .map(|a| Item {
                    label: a.name.to_string(),
                    detail: a.desc.to_string(),
                    run: Box::new(move || match quick::run_argv(a.id) {
                        Some(argv) => Outcome::Shell(argv, a.privileged),
                        None => Outcome::Inline("unknown action".into()),
                    }),
                })
                .collect(),
            3 => cleanup::catalog()
                .iter()
                .map(|t| Item {
                    label: t.name.to_string(),
                    detail: t.desc.to_string(),
                    run: Box::new(move || {
                        if t.elevated {
                            match cleanup::clean_cmd(t.id) {
                                Some(c) => Outcome::Shell(c.argv, true),
                                None => Outcome::Inline("nothing to do".into()),
                            }
                        } else {
                            let _ = cleanup::clean(t.id);
                            Outcome::Inline(format!("cleaned {}", t.name))
                        }
                    }),
                })
                .collect(),
            4 => debloat::catalog()
                .iter()
                .map(|b| {
                    let installed = debloat::pkg_name(b)
                        .map(debloat::is_installed)
                        .unwrap_or(false);
                    Item {
                        label: format!(
                            "{} ({})",
                            b.name,
                            if installed { "installed" } else { "absent" }
                        ),
                        detail: b.desc.to_string(),
                        run: Box::new(move || match debloat::remove_cmd(b) {
                            Some(c) => Outcome::Shell(c.argv, true),
                            None => Outcome::Inline("not removable here".into()),
                        }),
                    }
                })
                .collect(),
            5 => power_items(),
            6 => dns::providers()
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let link = link.to_string();
                    Item {
                        label: p.name.to_string(),
                        detail: format!("apply on {link}"),
                        run: Box::new(move || match dns::set_cmd(i, &link) {
                            Some(c) => Outcome::Shell(c.argv, true),
                            None => Outcome::Inline("n/a".into()),
                        }),
                    }
                })
                .collect(),
            7 => firewall_items(),
            8 => autostart::entries()
                .into_iter()
                .map(|e| {
                    let file = e.file.clone();
                    let enabled = e.enabled;
                    Item {
                        label: format!("[{}] {}", if enabled { "x" } else { " " }, e.name),
                        detail: if e.system {
                            "system".into()
                        } else {
                            "user".into()
                        },
                        run: Box::new(move || {
                            let _ = autostart::set_enabled(&file, !enabled);
                            Outcome::Inline(format!(
                                "{} {}",
                                if !enabled { "enabled" } else { "disabled" },
                                file
                            ))
                        }),
                    }
                })
                .collect(),
            9 => match restore::detect() {
                restore::Tool::None => {
                    vec![info("No snapshot tool (install Timeshift or Snapper)")]
                }
                _ => vec![Item {
                    label: "Create snapshot".into(),
                    detail: "comment: NeonPrime".into(),
                    run: Box::new(|| match restore::create_cmd("NeonPrime") {
                        Some(c) => Outcome::Shell(c.argv, true),
                        None => Outcome::Inline("n/a".into()),
                    }),
                }],
            },
            10 => packages_items(mgr),
            11 => services::services()
                .into_iter()
                .map(|s| {
                    let name = s.name.clone();
                    let running = s.running;
                    let title = if s.description.is_empty() {
                        s.name.clone()
                    } else {
                        s.description.clone()
                    };
                    Item {
                        label: format!("[{}] {}", if running { "on" } else { "  " }, title),
                        detail: s.name.clone(),
                        run: Box::new(move || {
                            let c = if running {
                                services::stop(&name)
                            } else {
                                services::start(&name)
                            };
                            Outcome::Shell(c.argv, true)
                        }),
                    }
                })
                .collect(),
            12 => graphics_items(),
            13 => servers_items(),
            _ => Vec::new(),
        }
    }

    fn graphics_items() -> Vec<Item> {
        let gpus = gpudriver::detect();
        let summary = gpus
            .iter()
            .map(|g| g.vendor.label())
            .collect::<Vec<_>>()
            .join(" + ");
        let mut items = vec![info(format!("GPUs: {summary}"))];
        if gaming::is_hybrid() {
            items.push(info(format!(
                "dGPU launch options: {}",
                gaming::launch_options()
            )));
        }
        for v in [
            gpudriver::Vendor::Nvidia,
            gpudriver::Vendor::Amd,
            gpudriver::Vendor::Intel,
        ] {
            if gpus.iter().any(|g| g.vendor == v) {
                if let Some(c) = gpudriver::install_cmd(v) {
                    let argv = c.argv.clone();
                    items.push(Item {
                        label: format!("Install {} driver/userspace", v.label()),
                        detail: c.summary.clone(),
                        run: Box::new(move || Outcome::Shell(argv.clone(), true)),
                    });
                }
            }
        }
        if let Some(c) = gaming::install_tools_cmd() {
            let argv = c.argv.clone();
            items.push(Item {
                label: "Install gaming tools".into(),
                detail: c.summary.clone(),
                run: Box::new(move || Outcome::Shell(argv.clone(), true)),
            });
        }
        if let Some(c) = gaming::switcheroo_cmd() {
            let argv = c.argv.clone();
            items.push(Item {
                label: "Enable per-app GPU switching".into(),
                detail: "switcheroo-control (right-click 'Run with dedicated GPU')".into(),
                run: Box::new(move || Outcome::Shell(argv.clone(), true)),
            });
        }
        items
    }

    fn servers_items() -> Vec<Item> {
        servers::catalog()
            .iter()
            .map(|s| {
                let st = servers::status(s);
                let state = if !st.installed {
                    "absent"
                } else if st.running {
                    "running"
                } else {
                    "stopped"
                };
                let disable = st.enabled && st.installed;
                let cmd = if disable {
                    Some(servers::disable_cmd(s))
                } else {
                    servers::install_enable_cmd(s)
                };
                match cmd {
                    Some(c) => {
                        let argv = c.argv.clone();
                        let verb = if disable { "disable" } else { "enable" };
                        Item {
                            label: format!("[{state}] {} ({verb})", s.name),
                            detail: s.desc.to_string(),
                            run: Box::new(move || Outcome::Shell(argv.clone(), true)),
                        }
                    }
                    None => info(format!("{} (no package manager)", s.name)),
                }
            })
            .collect()
    }

    fn tweaks_items() -> Vec<Item> {
        if !tweaks::tools_available(tweaks::detect()) {
            return vec![info(format!(
                "{}: no settings tool found",
                tweaks::detect().label()
            ))];
        }
        tweaks::for_current()
            .into_iter()
            .map(|t| {
                let on = tweaks::is_applied(t);
                let priv_ = tweaks::privileged(t);
                let detail = if t.warn.is_empty() {
                    t.desc.to_string()
                } else {
                    format!("{}   [!] {}", t.desc, t.warn)
                };
                Item {
                    label: format!("[{}] {}", if on { "x" } else { " " }, t.name),
                    detail,
                    run: Box::new(move || Outcome::Shell(tweaks::apply_argv(t, !on), priv_)),
                }
            })
            .collect()
    }

    fn power_items() -> Vec<Item> {
        if !power::available() {
            return vec![info("power-profiles-daemon not found")];
        }
        let active = power::active();
        power::profiles()
            .iter()
            .map(|p| {
                let is_active = active.as_deref() == Some(p.id);
                Item {
                    label: format!("[{}] {}", if is_active { "*" } else { " " }, p.name),
                    detail: String::new(),
                    run: Box::new(move || Outcome::Shell(power::set_argv(p.id), false)),
                }
            })
            .collect()
    }

    fn firewall_items() -> Vec<Item> {
        if !firewall::available() {
            return vec![info("ufw is not installed")];
        }
        let state = match firewall::enabled() {
            Some(true) => "ACTIVE",
            Some(false) => "INACTIVE",
            None => "unknown (needs root to read)",
        };
        vec![
            info(format!("Firewall is {state}")),
            Item {
                label: "Enable".into(),
                detail: String::new(),
                run: Box::new(|| Outcome::Shell(firewall::enable().argv, true)),
            },
            Item {
                label: "Disable".into(),
                detail: String::new(),
                run: Box::new(|| Outcome::Shell(firewall::disable().argv, true)),
            },
            Item {
                label: "Reset".into(),
                detail: String::new(),
                run: Box::new(|| Outcome::Shell(firewall::reset().argv, true)),
            },
        ]
    }

    fn packages_items(mgr: Option<pkg::Manager>) -> Vec<Item> {
        let Some(m) = mgr else {
            return vec![info("No package manager found")];
        };
        apps::catalog()
            .iter()
            .filter_map(|a| {
                let id = apps::pkg_id(a, m)?;
                let priv_ = pkg::needs_privilege(m);
                Some(Item {
                    label: format!("{} ({id})", a.name),
                    detail: a.desc.to_string(),
                    run: Box::new(move || Outcome::Shell(pkg::install(m, id).argv, priv_)),
                })
            })
            .collect()
    }

    struct App {
        cat_i: usize,
        cat_state: ListState,
        item_state: ListState,
        items: Vec<Item>,
        status: String,
        mgr: Option<pkg::Manager>,
        link: String,
        specs: Vec<String>,
        tele: telemetry::Telemetry,
        sample: telemetry::Sample,
    }

    impl App {
        fn new() -> Self {
            let mgr = pkg::primary().or_else(|| pkg::detected().first().copied());
            let link = dns::default_link().unwrap_or_else(|| "none".into());
            let mut cat_state = ListState::default();
            cat_state.select(Some(0));
            let mut app = App {
                cat_i: 0,
                cat_state,
                item_state: ListState::default(),
                items: Vec::new(),
                status: "Ready. Arrow keys to navigate, Enter to run, q to quit.".into(),
                mgr,
                link,
                specs: gather_specs(),
                tele: telemetry::Telemetry::new(),
                sample: telemetry::Sample::default(),
            };
            app.rebuild();
            app
        }

        fn rebuild(&mut self) {
            self.items = build_items(self.cat_i, self.mgr, &self.link);
            let sel = if self.items.is_empty() { None } else { Some(0) };
            self.item_state.select(sel);
        }

        fn next_cat(&mut self) {
            self.cat_i = (self.cat_i + 1) % CATS.len();
            self.cat_state.select(Some(self.cat_i));
            self.rebuild();
        }

        fn prev_cat(&mut self) {
            self.cat_i = (self.cat_i + CATS.len() - 1) % CATS.len();
            self.cat_state.select(Some(self.cat_i));
            self.rebuild();
        }

        fn move_item(&mut self, delta: i32) {
            if self.items.is_empty() {
                return;
            }
            let n = self.items.len() as i32;
            let cur = self.item_state.selected().unwrap_or(0) as i32;
            let next = (cur + delta).rem_euclid(n);
            self.item_state.select(Some(next as usize));
        }

        fn activate(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
            let Some(i) = self.item_state.selected() else {
                return Ok(());
            };
            let Some(item) = self.items.get(i) else {
                return Ok(());
            };
            match (item.run)() {
                Outcome::Inline(msg) => {
                    if !msg.is_empty() {
                        self.status = msg;
                    }
                    self.rebuild();
                }
                Outcome::Shell(argv, privileged) => {
                    ratatui::restore();
                    run_shell(&argv, privileged);
                    *terminal = ratatui::init();
                    self.rebuild();
                    self.status = "Command finished.".into();
                }
            }
            Ok(())
        }

        fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
            loop {
                if self.cat_i == 0 {
                    self.sample = self.tele.sample();
                }
                terminal.draw(|f| self.render(f))?;
                if event::poll(std::time::Duration::from_millis(500))? {
                    if let Event::Key(k) = event::read()? {
                        if k.kind != KeyEventKind::Press {
                            continue;
                        }
                        match k.code {
                            KeyCode::Char('q') | KeyCode::Esc => break,
                            KeyCode::Up => self.move_item(-1),
                            KeyCode::Down => self.move_item(1),
                            KeyCode::Left => self.prev_cat(),
                            KeyCode::Right | KeyCode::Tab => self.next_cat(),
                            KeyCode::Enter => self.activate(terminal)?,
                            _ => {}
                        }
                    }
                }
            }
            Ok(())
        }

        fn render(&mut self, f: &mut Frame) {
            let cyan = Style::new().fg(Color::Cyan);
            let outer = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(0), Constraint::Length(1)])
                .split(f.area());
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(20), Constraint::Min(0)])
                .split(outer[0]);

            // Category list.
            let cat_items: Vec<ListItem> = CATS.iter().map(|c| ListItem::new(*c)).collect();
            let cats = List::new(cat_items)
                .block(Block::default().borders(Borders::ALL).title(" NEONPRIME "))
                .highlight_style(Style::new().fg(Color::Black).bg(Color::Cyan))
                .highlight_symbol("> ");
            f.render_stateful_widget(cats, cols[0], &mut self.cat_state);

            // Right pane.
            if self.cat_i == 0 {
                let s = &self.sample;
                let mut lines = vec![
                    metric_line("CPU", &format!("{:.0}%", s.cpu)),
                    metric_line(
                        "Memory",
                        &format!("{:.1} / {:.0} GiB", gib(s.mem_used), gib(s.mem_total)),
                    ),
                    metric_line(
                        "CPU temp",
                        &s.cpu_temp
                            .map(|t| format!("{t:.0} C"))
                            .unwrap_or_else(|| "N/A".into()),
                    ),
                    metric_line("Load (1m)", &format!("{:.2}", s.load1)),
                    Line::from(""),
                ];
                for spec in &self.specs {
                    lines.push(Line::from(Span::styled(
                        spec.clone(),
                        Style::new().fg(Color::DarkGray),
                    )));
                }
                let p = Paragraph::new(lines)
                    .block(Block::default().borders(Borders::ALL).title(" Dashboard "));
                f.render_widget(p, cols[1]);
            } else {
                let right = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Min(0), Constraint::Length(3)])
                    .split(cols[1]);
                let items: Vec<ListItem> = self
                    .items
                    .iter()
                    .map(|it| ListItem::new(it.label.clone()))
                    .collect();
                let list = List::new(items)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(format!(" {} ", CATS[self.cat_i])),
                    )
                    .highlight_style(cyan.add_modifier(Modifier::REVERSED))
                    .highlight_symbol("> ");
                f.render_stateful_widget(list, right[0], &mut self.item_state);

                let detail = self
                    .item_state
                    .selected()
                    .and_then(|i| self.items.get(i))
                    .map(|it| it.detail.clone())
                    .unwrap_or_default();
                let d = Paragraph::new(detail)
                    .wrap(Wrap { trim: true })
                    .block(Block::default().borders(Borders::ALL));
                f.render_widget(d, right[1]);
            }

            // Footer: status + help.
            let help = " up/down select  left/right tab  enter run  q quit ";
            let footer = Line::from(vec![
                Span::styled(format!(" {} ", self.status), cyan),
                Span::styled(help, Style::new().fg(Color::DarkGray)),
            ]);
            f.render_widget(Paragraph::new(footer), outer[1]);
        }
    }

    fn metric_line(label: &str, value: &str) -> Line<'static> {
        Line::from(vec![
            Span::styled(format!("{label:<10}"), Style::new().fg(Color::DarkGray)),
            Span::styled(value.to_string(), Style::new().fg(Color::Cyan)),
        ])
    }

    fn gib(bytes: u64) -> f64 {
        bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    }

    fn gather_specs() -> Vec<String> {
        use sysinfo::System;
        let mut sys = System::new_all();
        sys.refresh_cpu_usage();
        let cpu = sys
            .cpus()
            .first()
            .map(|c| c.brand().trim().to_string())
            .unwrap_or_default();
        vec![
            format!("OS      {}", System::long_os_version().unwrap_or_default()),
            format!("Kernel  {}", System::kernel_version().unwrap_or_default()),
            format!("CPU     {cpu}"),
        ]
    }

    /// Suspend-and-run: the TUI is already down (caller restored the terminal).
    fn run_shell(argv: &[String], privileged: bool) {
        if argv.is_empty() {
            return;
        }
        let shown = if privileged {
            format!("sudo {}", argv.join(" "))
        } else {
            argv.join(" ")
        };
        println!("\n> {shown}\n");
        let status = if privileged {
            Command::new("sudo").args(argv).status()
        } else {
            Command::new(&argv[0]).args(&argv[1..]).status()
        };
        match status {
            Ok(s) => println!("\n[exit {}]", s.code().unwrap_or(-1)),
            Err(e) => println!("\n[failed: {e}]"),
        }
        print!("\nPress Enter to return to NeonPrime... ");
        let _ = io::stdout().flush();
        let mut s = String::new();
        let _ = io::stdin().read_line(&mut s);
    }

    pub fn run() -> io::Result<()> {
        let mut terminal = ratatui::init();
        let mut app = App::new();
        let res = app.run(&mut terminal);
        ratatui::restore();
        res
    }
}
