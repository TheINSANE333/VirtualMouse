use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

fn find_usbip_executable() -> Option<PathBuf> {
    // 1. Check PATH
    if let Ok(output) = Command::new("usbip").arg("-V").output() {
        if output.status.success() {
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
            return Some(path.to_path_buf());
        }
    }

    // 3. Check %LOCALAPPDATA%
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let candidate = PathBuf::from(local_app_data).join(r"Programs\usbip\usbip.exe");
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}

fn prompt_install_driver() {
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
        println!("\n[*] Installing USB/IP driver via winget...");
        let status = Command::new("winget")
            .args(["install", "vadimgrn.usbip-win2", "--accept-package-agreements", "--accept-source-agreements"])
            .status();

        match status {
            Ok(s) if s.success() => {
                println!("\n[+] USB/IP driver installed successfully!");
            }
            _ => {
                println!("\n[!] Automatic installation failed or was cancelled.");
                println!("    Please run: winget install vadimgrn.usbip-win2");
                println!("    Or download from: https://github.com/vadimgrn/usbip-win2/releases");
            }
        }
    } else {
        println!("\nTo install manually, run:");
        println!("    winget install vadimgrn.usbip-win2");
    }
}

fn list_ports(usbip: &Path) -> Vec<String> {
    let mut ports = Vec::new();
    if let Ok(output) = Command::new(usbip).arg("port").output() {
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            let line_trimmed = line.trim();
            if line_trimmed.starts_with("Port") || line_trimmed.starts_with("port") {
                // E.g. "Port 00: <Port in Use> ..."
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

fn print_ports(usbip: &Path) {
    if let Ok(output) = Command::new(usbip).arg("port").output() {
        let text = String::from_utf8_lossy(&output.stdout);
        if text.trim().is_empty() {
            println!("  (No active USB/IP ports currently attached)");
        } else {
            for line in text.lines() {
                println!("  {}", line);
            }
        }
    }
}

fn detach_port(usbip: &Path, port: &str) -> bool {
    println!("[*] Detaching USB/IP port {}...", port);
    let status = Command::new(usbip)
        .args(["detach", "-p", port])
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("[+] Port {} detached successfully.", port);
            true
        }
        _ => {
            println!("[!] Failed to detach port {}.", port);
            false
        }
    }
}

fn detach_all_ports(usbip: &Path) {
    let ports = list_ports(usbip);
    if ports.is_empty() {
        // Try detaching default port 00 just in case
        let _ = Command::new(usbip).args(["detach", "-p", "00"]).status();
        let _ = Command::new(usbip).args(["detach", "-p", "01"]).status();
    } else {
        for p in ports {
            detach_port(usbip, &p);
        }
    }
}

fn list_devices_on_server(usbip: &Path, server_ip: &str) -> bool {
    println!("[*] Querying shared devices on {}:3240...", server_ip);
    match Command::new(usbip).args(["list", "-r", server_ip]).output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stdout.trim().is_empty() {
                println!("{}", stdout);
                true
            } else {
                if !stderr.trim().is_empty() {
                    eprintln!("[!] Server response error: {}", stderr.trim());
                } else {
                    println!("  (No exportable devices found on {})", server_ip);
                }
                false
            }
        }
        Err(e) => {
            eprintln!("[!] Failed to query server {}: {}", server_ip, e);
            false
        }
    }
}

fn attach_device(usbip: &Path, server_ip: &str, busid: &str) -> bool {
    println!("[*] Attaching remote mouse ({}) from {}...", busid, server_ip);
    let status = Command::new(usbip)
        .args(["attach", "-r", server_ip, "-b", busid])
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("[+] Attached successfully! Windows is registering the physical USB mouse.");
            true
        }
        _ => {
            eprintln!("[!] Attach failed. Ensure VirtualMouseServer.exe is running on {}", server_ip);
            false
        }
    }
}

fn print_banner() {
    println!("\n==================================================================");
    println!("  VirtualMouse Host Controller (Client)");
    println!("  Genuine Physical USB Mouse Receiver via USB/IP");
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

    // 1. Locate usbip.exe
    let usbip_path = match find_usbip_executable() {
        Some(p) => p,
        None => {
            prompt_install_driver();
            match find_usbip_executable() {
                Some(p) => p,
                None => {
                    println!("\n[!] Please install the USB/IP driver and run VirtualMouseHost.exe again.");
                    println!("Press [Enter] to exit...");
                    let mut dummy = String::new();
                    let _ = io::stdin().read_line(&mut dummy);
                    return;
                }
            }
        }
    };

    println!("[*] USB/IP Client Driver: {}", usbip_path.display());

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
                print_ports(&usbip_path);
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
            detach_port(&usbip_path, p);
        } else {
            detach_all_ports(&usbip_path);
        }
        return;
    }

    if list_mode {
        let ip = server_ip.unwrap_or_else(|| "127.0.0.1".to_string());
        list_devices_on_server(&usbip_path, &ip);
        return;
    }

    // 3. Interactive Mode if no IP specified
    let target_ip = match server_ip {
        Some(ip) => ip,
        None => {
            // Check existing ports
            let existing_ports = list_ports(&usbip_path);
            if !existing_ports.is_empty() {
                println!("\n[!] Found active USB/IP ports already imported:");
                print_ports(&usbip_path);
                print!("\nWould you like to detach them before connecting? [Y/n]: ");
                io::stdout().flush().unwrap();
                let mut answer = String::new();
                let _ = io::stdin().read_line(&mut answer);
                let trimmed = answer.trim();
                if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("y") {
                    for p in existing_ports {
                        detach_port(&usbip_path, &p);
                    }
                }
            }

            print!("\nEnter Server PC IP address [default: 127.0.0.1]: ");
            io::stdout().flush().unwrap();
            let mut input = String::new();
            let _ = io::stdin().read_line(&mut input);
            let trimmed = input.trim();
            if trimmed.is_empty() {
                "127.0.0.1".to_string()
            } else {
                trimmed.to_string()
            }
        }
    };

    println!();
    list_devices_on_server(&usbip_path, &target_ip);

    // 4. Attach mouse (busid: 1-1)
    if !attach_device(&usbip_path, &target_ip, "1-1") {
        println!("\nPress [Enter] to exit...");
        let mut dummy = String::new();
        let _ = io::stdin().read_line(&mut dummy);
        return;
    }

    println!("\n==================================================================");
    println!("  [●] VirtualMouse is ACTIVE and mapped to this Host PC!");
    println!("  Windows Device Manager registers this as a physical USB mouse.");
    println!("  Currently imported ports:");
    print_ports(&usbip_path);
    println!("==================================================================");
    println!("  Press [Enter] or Ctrl+C at any time to detach and exit.");
    println!("==================================================================\n");

    // Setup clean auto-detachment on Ctrl+C
    let usbip_cleanup = usbip_path.clone();
    setup_ctrlc(move || {
        println!("\n[*] Detaching USB mouse before exit...");
        detach_all_ports(&usbip_cleanup);
        std::process::exit(0);
    });

    // Wait for user to press Enter to exit
    let mut exit_buf = String::new();
    let _ = io::stdin().read_line(&mut exit_buf);

    println!("\n[*] Exiting... Detaching USB mouse cleanly.");
    detach_all_ports(&usbip_path);
    println!("[+] Detached. Goodbye!\n");
}
