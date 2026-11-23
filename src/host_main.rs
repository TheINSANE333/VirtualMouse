mod logger;
mod net_util;

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use logger::Logger;

fn find_usbip_executable(logger: &Arc<Logger>) -> Option<PathBuf> {
    logger.log_file_only("DRIVER", "Searching for usbip.exe...");

    // 1. Check PATH
    if let Ok(output) = Command::new("usbip").arg("-V").output() {
        if output.status.success() {
            logger.log_file_only("DRIVER", "Found usbip in PATH");
            return Some(PathBuf::from("usbip"));
        }
    }

    // 2. Check standard Windows installation paths
    let standard_paths = [
        r"C:\Program Files\usbip\usbip.exe",
        r"C:\Program Files\usbip-win2\usbip.exe",
        r"C:\Program Files (x86)\usbip\usbip.exe",
        r"C:\Program Files (x86)\usbip-win2\usbip.exe",
    ];

    for p in &standard_paths {
        let path = Path::new(p);
        if path.is_file() {
            logger.log_file_only("DRIVER", &format!("Found usbip at {}", p));
            return Some(path.to_path_buf());
        }
    }

    // 3. Check %LOCALAPPDATA%
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let candidate = PathBuf::from(local_app_data).join(r"Programs\usbip\usbip.exe");
        if candidate.is_file() {
            logger.log_file_only("DRIVER", &format!("Found usbip at {}", candidate.display()));
            return Some(candidate);
        }
    }

    logger.log("DRIVER_ERR", "usbip.exe not found in PATH or standard installation directories.");
    None
}

fn prompt_install_driver(logger: &Arc<Logger>) {
    println!("\n==================================================================");
    println!("  [!] USB/IP Driver not found on this Host PC.");
    println!("  Windows requires the USB/IP VHCI driver so that remote inputs");
    println!("  are registered as an authentic physical USB mouse in Device Manager.");
    println!("==================================================================");
    println!("\nWould you like to install vadimgrn.usbip-win2 via winget now? [Y/n]: ");
    io::stdout().flush().unwrap();

    let mut input = String::new();
    let _ = io::stdin().read_line(&mut input);
    let trimmed = input.trim();

    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("y") {
        logger.log("INSTALL", "Installing USB/IP driver via winget...");
        let status = Command::new("winget")
            .args(["install", "vadimgrn.usbip-win2", "--accept-package-agreements", "--accept-source-agreements"])
            .status();

        match status {
            Ok(s) if s.success() => {
                logger.log("INSTALL", "USB/IP driver installed successfully!");
            }
            _ => {
                logger.log("INSTALL_ERR", "Automatic installation failed or was cancelled.");
                println!("    Please run: winget install vadimgrn.usbip-win2");
                println!("    Or download from: https://github.com/vadimgrn/usbip-win2/releases");
            }
        }
    } else {
        logger.log("INSTALL", "User opted to install driver manually.");
        println!("\nTo install manually, run:");
        println!("    winget install vadimgrn.usbip-win2");
    }
}

fn list_ports(usbip: &Path, logger: &Arc<Logger>) -> Vec<String> {
    let mut ports = Vec::new();
    if let Ok(output) = Command::new(usbip).arg("port").output() {
        let text = String::from_utf8_lossy(&output.stdout);
        logger.log_file_only("PORT_RAW", &text);
        for line in text.lines() {
            let line_trimmed = line.trim();
            if line_trimmed.starts_with("Port") || line_trimmed.starts_with("port") {
                if let Some(rest) = line_trimmed.strip_prefix("Port ").or_else(|| line_trimmed.strip_prefix("port ")) {
                    if let Some(port_id) = rest.split(':').next() {
                        ports.push(port_id.trim().to_string());
                    }
                }
            }
        }
    }
    ports
}

fn print_ports(usbip: &Path, logger: &Arc<Logger>) {
    if let Ok(output) = Command::new(usbip).arg("port").output() {
        let text = String::from_utf8_lossy(&output.stdout);
        logger.log_file_only("PORTS", &text);
        if text.trim().is_empty() {
            println!("  (No active USB/IP ports currently attached)");
        } else {
            for line in text.lines() {
                println!("  {}", line);
            }
        }
    }
}

fn detach_port(usbip: &Path, port: &str, logger: &Arc<Logger>) -> bool {
    logger.log("DETACH", &format!("Detaching USB/IP port {}...", port));
    let status = Command::new(usbip)
        .args(["detach", "-p", port])
        .output();

    match status {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            logger.log_file_only("DETACH_OUT", &format!("stdout: {}, stderr: {}", stdout.trim(), stderr.trim()));
            if out.status.success() {
                logger.log("DETACH", &format!("Port {} detached successfully.", port));
                true
            } else {
                logger.log("DETACH_ERR", &format!("Failed to detach port {}: {}", port, stderr.trim()));
                false
            }
        }
        Err(e) => {
            logger.log("DETACH_ERR", &format!("Failed to execute detach: {}", e));
            false
        }
    }
}

fn detach_all_ports(usbip: &Path, logger: &Arc<Logger>) {
    let ports = list_ports(usbip, logger);
    if ports.is_empty() {
        let _ = Command::new(usbip).args(["detach", "-p", "00"]).output();
        let _ = Command::new(usbip).args(["detach", "-p", "01"]).output();
    } else {
        for p in ports {
            detach_port(usbip, &p, logger);
        }
    }
}

fn list_devices_on_server(usbip: &Path, server_ip: &str, logger: &Arc<Logger>) -> bool {
    logger.log("QUERY", &format!("Querying shared USB devices on {}:3240...", server_ip));
    match Command::new(usbip).args(["list", "-r", server_ip]).output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            logger.log_file_only("QUERY_OUT", &format!("stdout:\n{}\nstderr:\n{}", stdout, stderr));

            if !stdout.trim().is_empty() {
                println!("{}", stdout);
                true
            } else {
                if !stderr.trim().is_empty() {
                    logger.log("QUERY_ERR", &format!("Server response error: {}", stderr.trim()));
                } else {
                    println!("  (No exportable devices found on {})", server_ip);
                }
                false
            }
        }
        Err(e) => {
            logger.log("QUERY_ERR", &format!("Failed to query server {}: {}", server_ip, e));
            false
        }
    }
}

fn attach_device(usbip: &Path, server_ip: &str, busid: &str, logger: &Arc<Logger>) -> bool {
    logger.log("ATTACH", &format!("Attaching remote mouse ({}) from {}...", busid, server_ip));
    match Command::new(usbip)
        .args(["attach", "-r", server_ip, "-b", busid])
        .output()
    {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            logger.log_file_only("ATTACH_OUT", &format!("stdout: {}\nstderr: {}", stdout.trim(), stderr.trim()));

            if out.status.success() {
                logger.log("ATTACH", "Attached successfully! Windows is registering the physical USB mouse.");
                true
            } else {
                logger.log("ATTACH_ERR", &format!("Attach failed: {}. Ensure VirtualMouseServer.exe is running on {}", stderr.trim(), server_ip));
                false
            }
        }
        Err(e) => {
            logger.log("ATTACH_ERR", &format!("Failed to execute attach: {}", e));
            false
        }
    }
}

fn check_windows_device_status(logger: &Arc<Logger>) {
    // Check if Windows registers the Logitech mouse (046D:C077)
    let ps_cmd = "Get-CimInstance Win32_PnPEntity | Where-Object { $_.DeviceID -like '*046D*C077*' -or $_.Name -like '*HID-compliant mouse*' } | Select-Object Name, DeviceID, Status | Format-Table -AutoSize | Out-String";
    if let Ok(out) = Command::new("powershell").args(["-NoProfile", "-Command", ps_cmd]).output() {
        let text = String::from_utf8_lossy(&out.stdout);
        logger.log_file_only("PNP_STATUS", &format!("Windows Device Manager status:\n{}", text.trim()));
    }
}

fn print_banner() {
    println!("\n==================================================================");
    println!("  VirtualMouse Host Controller (Client)");
    println!("  Genuine Physical USB Mouse Receiver via USB/IP");
    println!("  Log File: host.log (Detailed diagnostic log)");
    println!("==================================================================");
}

fn setup_ctrlc<F>(f: F)
where
    F: Fn() + Send + 'static,
{
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetConsoleCtrlHandler(
            HandlerRoutine: Option<unsafe extern "system" fn(u32) -> i32>,
            Add: i32,
        ) -> i32;
    }

    static mut HANDLER: Option<Box<dyn Fn() + Send>> = None;
    unsafe {
        HANDLER = Some(Box::new(f));
        unsafe extern "system" fn ctrl_handler(_ctrl_type: u32) -> i32 {
            unsafe {
                if let Some(ref h) = HANDLER {
                    h();
                }
            }
            1
        }
        SetConsoleCtrlHandler(Some(ctrl_handler), 1);
    }
}

fn main() {
    print_banner();

    let logger = Arc::new(Logger::new("host.log"));
    logger.log("MAIN", "VirtualMouseHost started.");

    // 1. Locate usbip.exe
    let usbip_path = match find_usbip_executable(&logger) {
        Some(p) => p,
        None => {
            prompt_install_driver(&logger);
            match find_usbip_executable(&logger) {
                Some(p) => p,
                None => {
                    logger.log("MAIN_ERR", "Please install the USB/IP driver and run VirtualMouseHost.exe again.");
                    println!("Press [Enter] to exit...");
                    let mut dummy = String::new();
                    let _ = io::stdin().read_line(&mut dummy);
                    return;
                }
            }
        }
    };

    logger.log("DRIVER", &format!("USB/IP Client Driver binary: {}", usbip_path.display()));

    // 2. Parse command-line arguments
    let args: Vec<String> = std::env::args().collect();
    let mut server_ip: Option<String> = None;
    let mut detach_mode = false;
    let mut detach_port_arg: Option<String> = None;
    let mut list_mode = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--detach" | "-d" => {
                detach_mode = true;
                if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                    detach_port_arg = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "--list" | "-l" => {
                list_mode = true;
                if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                    server_ip = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "--ports" | "-p" => {
                println!("\n[*] Currently imported USB/IP ports:");
                print_ports(&usbip_path, &logger);
                return;
            }
            "--help" | "-h" => {
                println!("Usage: VirtualMouseHost.exe [SERVER_IP] [OPTIONS]");
                println!("Options:");
                println!("  [SERVER_IP]        IP address of the Server PC to attach to");
                println!("  --detach, -d [PORT] Detach a specific port or all active ports");
                println!("  --list, -l [IP]    List exportable devices on server");
                println!("  --ports, -p        Show currently imported ports");
                return;
            }
            other => {
                if !other.starts_with('-') && server_ip.is_none() {
                    server_ip = Some(other.to_string());
                }
            }
        }
        i += 1;
    }

    if detach_mode {
        if let Some(ref p) = detach_port_arg {
            detach_port(&usbip_path, p, &logger);
        } else {
            detach_all_ports(&usbip_path, &logger);
        }
        return;
    }

    if list_mode {
        let ip = server_ip.unwrap_or_else(|| "127.0.0.1".to_string());
        list_devices_on_server(&usbip_path, &ip, &logger);
        return;
    }

    // 3. Determine target Server IP
    let target_ip = match server_ip {
        Some(ip) => ip,
        None => {
            // Check existing ports
            let existing_ports = list_ports(&usbip_path, &logger);
            if !existing_ports.is_empty() {
                println!("\n[!] Found active USB/IP ports already imported:");
                print_ports(&usbip_path, &logger);
                print!("\nWould you like to detach them before connecting? [Y/n]: ");
                io::stdout().flush().unwrap();
                let mut answer = String::new();
                let _ = io::stdin().read_line(&mut answer);
                let trimmed = answer.trim();
                if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("y") {
                    for p in existing_ports {
                        detach_port(&usbip_path, &p, &logger);
                    }
                }
            }

            // Display Host's own local adapters
            let adapters = net_util::get_local_adapters();
            if !adapters.is_empty() {
                println!("\n[*] Host PC Network Adapters:");
                for a in &adapters {
                    let kind = if a.is_ethernet { "Ethernet (High-Speed)" } else { "Wireless / Other" };
                    println!("    • {:<20} -> {} ({})", a.name, a.ip, kind);
                }
            }

            // Read last saved server IP
            let saved_ip_file = ".last_server_ip";
            let last_saved_ip = std::fs::read_to_string(saved_ip_file)
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());

            // Auto-discover Server over network
            print!("\n[*] Probing network for VirtualMouse Server (Ethernet & Wi-Fi)... ");
            io::stdout().flush().unwrap();
            let discovered = net_util::discover_server(500);

            let default_ip = if let Some((ref ip, is_eth)) = discovered {
                if is_eth {
                    println!("Found via Ethernet: {}!", ip);
                } else {
                    println!("Found: {}!", ip);
                }
                ip.clone()
            } else if let Some(ref ip) = last_saved_ip {
                println!("(Using last saved IP: {})", ip);
                ip.clone()
            } else {
                println!("(None detected)");
                "127.0.0.1".to_string()
            };

            print!("\nEnter Server PC IP address [default: {}]: ", default_ip);
            io::stdout().flush().unwrap();
            let mut input = String::new();
            let _ = io::stdin().read_line(&mut input);
            let trimmed = input.trim();
            let chosen = if trimmed.is_empty() {
                default_ip
            } else {
                trimmed.to_string()
            };

            let _ = std::fs::write(saved_ip_file, &chosen);
            chosen
        }
    };

    println!();
    list_devices_on_server(&usbip_path, &target_ip, &logger);

    // 4. Attach mouse (busid: 1-1)
    if !attach_device(&usbip_path, &target_ip, "1-1", &logger) {
        println!("\nPress [Enter] to exit...");
        let mut dummy = String::new();
        let _ = io::stdin().read_line(&mut dummy);
        return;
    }

    // Check Windows PnP device status
    check_windows_device_status(&logger);

    logger.log("ATTACH", "VirtualMouse is ACTIVE and mapped to this Host PC!");
    println!("\n==================================================================");
    println!("  [●] VirtualMouse is ACTIVE and mapped to this Host PC!");
    println!("  Windows Device Manager registers this as a physical USB mouse.");
    println!("  Currently imported ports:");
    print_ports(&usbip_path, &logger);
    println!("==================================================================");
    println!("  Press [Enter] or Ctrl+C at any time to detach and exit.");
    println!("==================================================================\n");

    // Setup clean auto-detachment on Ctrl+C
    let usbip_cleanup = usbip_path.clone();
    let log_cleanup = logger.clone();
    setup_ctrlc(move || {
        log_cleanup.log("CLEANUP", "Detaching USB mouse before exit (Ctrl+C)...");
        detach_all_ports(&usbip_cleanup, &log_cleanup);
        std::process::exit(0);
    });

    // Wait for user to press Enter to exit
    let mut exit_buf = String::new();
    let _ = io::stdin().read_line(&mut exit_buf);

    logger.log("MAIN", "User pressed Enter. Detaching cleanly and exiting...");
    detach_all_ports(&usbip_path, &logger);
    logger.log("MAIN", "Detached. Goodbye!");
    println!("[+] Detached. Goodbye!\n");
}
