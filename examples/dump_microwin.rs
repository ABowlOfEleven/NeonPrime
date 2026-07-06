//! Dev helper: dump the generated MicroWin build script + autounattend so their
//! syntax can be validated (the real build can't run in CI). Not shipped.
//! Windows-only (MicroWin is part of the Windows backend).

#[cfg(windows)]
fn main() {
    use neonprime::core::microwin::{build_script, Options, AUTOUNATTEND};
    let o = Options {
        iso: "D:\\win11.iso".into(),
        output: "D:\\win11-NeonPrime.iso".into(),
        scratch: "C:\\NeonPrime-MicroWin".into(),
        index: 1,
        debloat: true,
        privacy: true,
        bypass: true,
    };
    let dir = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    std::fs::write(
        format!("{dir}/microwin.ps1"),
        build_script(&o, "C:\\adk\\oscdimg.exe", "C:\\t\\unattend.xml"),
    )
    .unwrap();
    std::fs::write(format!("{dir}/autounattend.xml"), AUTOUNATTEND).unwrap();
    println!("wrote microwin.ps1 + autounattend.xml to {dir}");
}

#[cfg(not(windows))]
fn main() {
    eprintln!("dump_microwin is Windows-only (MicroWin is the Windows backend).");
}
