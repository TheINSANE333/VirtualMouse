mod capture;
mod descriptors;
mod usbip;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::sync_channel;
use std::sync::Arc;

use capture::start_raw_input_loop;
use usbip::{MouseReport, UsbIpServer, USBIP_PORT};

fn print_banner(host: &str, port: u16, delay: u64) {
    println!("\n==================================================================");
    println!("  VirtualMouse USB/IP Server (Native Rust VirtualHere Engine)");
    println!("  ZERO GUI WINDOWS | NO CURSOR WARPING | PURE HARDWARE DELTAS");
    println!("==================================================================");
    println!("  Server Address : {}:{}", host, port);
    println!("  Countdown Delay: {} seconds", delay);
    println!("  Toggle Keys    : [ F12 ] or [ F11 ] or [ Scroll Lock ]");
    println!("                   (or simply press [Enter] in this console)");
    println!("==================================================================");
    println!("  Workflow:");
    println!("  1. Connect Host PC via usbip attach");
    println!("  2. Press [ F12 ] -> hear {} countdown beeps", delay);
    println!("  3. Switch to your full-screen Remote Desktop window");
    println!("  4. When you hear the high chime, your physical mouse is mapped!");
    println!("  5. Press [ F12 ] again anytime to release mouse back to server.");
    println!("==================================================================\n");
}

fn main() {
    let mut host = "0.0.0.0".to_string();
    let mut port = USBIP_PORT;
    let mut delay: u64 = 5;

    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--host" => {
                if i + 1 < args.len() {
                    host = args[i + 1].clone();
                    i += 1;
                }
            }
            "--port" => {
                if i + 1 < args.len() {
                    if let Ok(p) = args[i + 1].parse() {
                        port = p;
                    }
                    i += 1;
                }
            }
            "--delay" => {
                if i + 1 < args.len() {
                    if let Ok(d) = args[i + 1].parse() {
                        delay = d;
                    }
                    i += 1;
                }
            }
            "--help" | "-h" => {
                println!("Usage: virtual_mouse_server [OPTIONS]");
                println!("Options:");
                println!("  --host <IP>      Bind address (default: 0.0.0.0)");
                println!("  --port <PORT>    Bind port (default: 3240)");
                println!("  --delay <SECS>   Countdown delay in seconds (default: 5)");
                return;
            }
            _ => {}
        }
        i += 1;
    }

    print_banner(&host, port, delay);

    let is_running = Arc::new(AtomicBool::new(true));
    let is_captured = Arc::new(AtomicBool::new(false));
    let is_counting_down = Arc::new(AtomicBool::new(false));

    // Handle Ctrl+C
    let r = is_running.clone();
    let _ = ctrlc_handler(move || {
        println!("\n[!] Shutting down VirtualMouse server...");
        r.store(false, Ordering::SeqCst);
        std::process::exit(0);
    });

    let (mouse_tx, mouse_rx) = sync_channel::<MouseReport>(2048);

    // Spawn USB/IP Server in background thread
    let mut server = UsbIpServer::new(&host, port);
    let s_running = is_running.clone();
    server.is_running = s_running;

    std::thread::spawn(move || {
        server.start(mouse_rx);
    });

    // Run Raw Input Window Message Pump on main thread
    start_raw_input_loop(
        mouse_tx,
        is_captured,
        is_counting_down,
        is_running,
        delay,
    );
}

fn ctrlc_handler<F>(f: F) -> Result<(), ()>
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
    Ok(())
}
