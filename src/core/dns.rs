//! DNS provider switcher. Setting DNS needs admin (applies to every active
//! adapter), so it runs through the elevated shell. "Automatic" resets to DHCP.

pub struct Provider {
    pub id: &'static str,
    pub name: &'static str,
    /// IPv4 servers, or empty for "Automatic" (reset to DHCP).
    pub servers: &'static [&'static str],
}

pub fn providers() -> &'static [Provider] {
    &[
        Provider { id: "auto", name: "AUTOMATIC", servers: &[] },
        Provider { id: "cloudflare", name: "CLOUDFLARE", servers: &["1.1.1.1", "1.0.0.1"] },
        Provider { id: "google", name: "GOOGLE", servers: &["8.8.8.8", "8.8.4.4"] },
        Provider { id: "quad9", name: "QUAD9", servers: &["9.9.9.9", "149.112.112.112"] },
    ]
}

/// Elevated PowerShell that points every "Up" adapter at the chosen provider
/// (or resets to DHCP) and flushes the resolver cache.
pub fn set_script(idx: usize) -> Option<String> {
    let p = providers().get(idx)?;
    let per_if = if p.servers.is_empty() {
        "Set-DnsClientServerAddress -InterfaceIndex $i -ResetServerAddresses".to_string()
    } else {
        let list = p.servers.iter().map(|s| format!("'{s}'")).collect::<Vec<_>>().join(",");
        format!("Set-DnsClientServerAddress -InterfaceIndex $i -ServerAddresses ({list})")
    };
    Some(format!(
        "$ifs = Get-NetAdapter -Physical | Where-Object Status -eq Up | Select-Object -ExpandProperty ifIndex; \
         foreach ($i in $ifs) {{ {per_if} }}; Clear-DnsClientCache; Write-Host 'DNS updated.'"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn providers_and_scripts() {
        assert!(providers().len() >= 3);
        assert!(set_script(0).unwrap().contains("ResetServerAddresses")); // auto
        assert!(set_script(1).unwrap().contains("1.1.1.1")); // cloudflare
        assert!(set_script(99).is_none());
    }
}
