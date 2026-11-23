use std::net::UdpSocket;
use std::process::Command;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct NetAdapter {
    pub name: String,
    pub ip: String,
    pub is_ethernet: bool,
}

pub fn get_local_adapters() -> Vec<NetAdapter> {
    let mut adapters = Vec::new();

    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "Get-NetIPAddress -AddressFamily IPv4 | Where-Object { $_.IPAddress -notlike '127.*' } | Select-Object -Property IPAddress, InterfaceAlias | ForEach-Object { \"$($_.InterfaceAlias)|$($_.IPAddress)\" }"
        ])
        .output();

    if let Ok(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines() {
            let line = line.trim();
            if let Some((alias, ip)) = line.split_once('|') {
                let alias = alias.trim();
                let ip = ip.trim();
                if !ip.is_empty() && !alias.is_empty() {
                    let is_eth = alias.to_lowercase().contains("ethernet")
                        || alias.to_lowercase().contains("eth")
                        || alias.to_lowercase().contains("lan");
                    adapters.push(NetAdapter {
                        name: alias.to_string(),
                        ip: ip.to_string(),
                        is_ethernet: is_eth,
                    });
                }
            }
        }
    }

    // Sort Ethernet adapters first
    adapters.sort_by(|a, b| b.is_ethernet.cmp(&a.is_ethernet));
    adapters
}

#[allow(dead_code)]
pub fn discover_server(timeout_ms: u64) -> Option<(String, bool)> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    let _ = socket.set_broadcast(true);
    let _ = socket.set_read_timeout(Some(Duration::from_millis(timeout_ms)));

    // Send broadcast probe to general broadcast and local subnets
    let _ = socket.send_to(b"VM_DISCOVER", "255.255.255.255:3240");
    let _ = socket.send_to(b"VM_DISCOVER", "127.0.0.1:3240");

    let adapters = get_local_adapters();
    for a in &adapters {
        if let Some((prefix, _)) = a.ip.rsplit_once('.') {
            let bcast = format!("{}.255:3240", prefix);
            let _ = socket.send_to(b"VM_DISCOVER", &bcast);
        }
    }

    let mut buf = [0u8; 64];
    if let Ok((len, src)) = socket.recv_from(&mut buf) {
        let text = String::from_utf8_lossy(&buf[..len]);
        if text.starts_with("VM_OFFER") {
            let ip = src.ip().to_string();
            // Check if this source IP matches any local Ethernet adapter subnet
            let is_eth = adapters.iter().any(|a| {
                a.is_ethernet && match (a.ip.rsplit_once('.'), ip.rsplit_once('.')) {
                    (Some((p1, _)), Some((p2, _))) => p1 == p2,
                    _ => false,
                }
            });
            return Some((ip, is_eth));
        }
    }

    None
}
