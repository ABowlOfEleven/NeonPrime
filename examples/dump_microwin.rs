//! Dev helper: dump the generated MicroWin build script + autounattend so their
//! syntax can be validated (the real build can't run in CI). Not shipped.
use neonprime::core::microwin::{build_script, Options, AUTOUNATTEND};

fn main() {
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
    std::fs::write(format!("{dir}/microwin.ps1"), build_script(&o, "C:\\adk\\oscdimg.exe", "C:\\t\\unattend.xml")).unwrap();
    std::fs::write(format!("{dir}/autounattend.xml"), AUTOUNATTEND).unwrap();
    println!("wrote microwin.ps1 + autounattend.xml to {dir}");
}
