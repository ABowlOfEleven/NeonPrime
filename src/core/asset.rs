//! Asset identity for the Support Bundle and the asset display: manufacturer,
//! model, serial / service tag, and a best-effort vendor warranty-lookup link.
//! Unelevated via CIM (Win32_ComputerSystem / Win32_BIOS).

use std::process::Command;

pub struct AssetInfo {
    pub manufacturer: String,
    pub model: String,
    pub serial: String,
    /// Vendor warranty-lookup URL (empty if the vendor is unknown or no serial).
    pub warranty_url: String,
}

pub fn info() -> AssetInfo {
    let out = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$c = Get-CimInstance Win32_ComputerSystem; $b = Get-CimInstance Win32_BIOS; \
             \"$($c.Manufacturer)`t$($c.Model)`t$($b.SerialNumber)\"",
        ])
        .output();
    let (mut manufacturer, mut model, mut serial) = (String::new(), String::new(), String::new());
    if let Ok(o) = out {
        let line = String::from_utf8_lossy(&o.stdout);
        let mut p = line.trim().splitn(3, '\t');
        manufacturer = p.next().unwrap_or("").trim().to_string();
        model = p.next().unwrap_or("").trim().to_string();
        serial = p.next().unwrap_or("").trim().to_string();
    }
    let warranty_url = warranty_link(&manufacturer, &serial);
    AssetInfo {
        manufacturer,
        model,
        serial,
        warranty_url,
    }
}

/// Map a manufacturer + serial to the vendor's warranty-lookup page. Dell and
/// Lenovo accept the tag in the URL; HP needs it typed on the page.
pub fn warranty_link(manufacturer: &str, serial: &str) -> String {
    let m = manufacturer.to_lowercase();
    let s = serial.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("System Serial Number") {
        return String::new();
    }
    if m.contains("dell") {
        format!("https://www.dell.com/support/home/en-us/product-support/servicetag/{s}/overview")
    } else if m.contains("lenovo") {
        format!("https://pcsupport.lenovo.com/us/en/warrantylookup?serial={s}")
    } else if m.contains("hp") || m.contains("hewlett") {
        "https://support.hp.com/us-en/check-warranty".to_string()
    } else if m.contains("microsoft") {
        "https://account.microsoft.com/devices".to_string()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendor_links() {
        assert!(warranty_link("Dell Inc.", "ABC1234").contains("servicetag/ABC1234"));
        assert!(warranty_link("LENOVO", "PF0ABCDE").contains("serial=PF0ABCDE"));
        assert!(warranty_link("HP", "X").contains("hp.com"));
    }

    #[test]
    fn no_serial_no_link() {
        assert!(warranty_link("Dell Inc.", "").is_empty());
        assert!(warranty_link("Acme", "Z").is_empty());
    }

    #[test]
    fn reads_asset() {
        let a = info();
        // Manufacturer or model is populated on real hardware/VMs.
        assert!(!a.manufacturer.is_empty() || !a.model.is_empty());
    }
}
