# VirtualMouse: Native Rust USB-over-IP Mouse

A minimalist, high-performance, open-source alternative to **VirtualHere** built entirely in **Rust**. Both the Server and Host components run as standalone, zero-dependency Windows **`.exe`** executables.

It maps a physical mouse from a Server PC to a Windows Host PC as a **genuine physical hardware USB mouse** (`HID-compliant mouse`) over the network using the open **USB/IP** protocol.

---

## Key Features

- **Both Server and Host Run as Standalone `.exe`:**
  - `VirtualMouseServer.exe` (run on Server PC)
  - `VirtualMouseHost.exe` (run on Host PC)
- **VirtualHere-Style Architecture (Zero GUI Windows):**
  - **No visible windows or overlay blocks:** Runs silently in the background using an invisible Windows Raw Input receiver. It will **never block your view** on Remote Desktop or any other application.
  - **No cursor trapping or synthetic cursor warping:** Directly reads raw optical sensor deltas (`lLastX`, `lLastY`, buttons, and wheel) via Windows Raw Input (`RIDEV_INPUTSINK`).
- **Global Kernel-Level Hotkey & Countdown Delay:**
  - Press **`[ F12 ]`**, **`[ F11 ]`**, or **`[ Scroll Lock ]`** from anywhere (or hit **`[Enter]`** in the console).
  - Configurable countdown timer (default 5 seconds) with audio chimes gives you time to switch and focus your full-screen Remote Desktop window before raw capture begins.
  - Press the hotkey again anytime to instantly release control back to your local Server PC.
- **Genuine Physical Hardware USB Mouse:**
  - Appears in Windows Device Manager on the Host PC with real Vendor ID (`0x046D`) and Product ID (`0xC077`) (Logitech USB Optical Mouse).
  - Compatible with games, anticheat systems, and secure desktops that reject synthetic `SendInput` events.
- **Ultra-Fast Native Rust Binaries:**
  - Sub-millisecond latency, zero garbage collection, zero Python runtime dependencies.

---

## Architecture

```
+---------------------------------------+                      +---------------------------------------+
|               Server PC               |                      |            Host PC (Windows)          |
|                                       |                      |                                       |
|  [ Physical Optical Mouse ]           |    TCP Port 3240     |       VirtualMouseHost.exe            |
|              | (Raw deltas)           |       (USB/IP)       |      +--------------------------+     |
|              v                        | -------------------> |      |    usbip-win2 Driver     |     |
|   VirtualMouseServer.exe              |                      |  <-- |   (Kernel Virtual Host   |     |
|   - Windowless Raw Input Receiver     |                      |      |    Controller Driver)    |     |
|   - Global Hotkey + Countdown Chimes  |                      |      +--------------------------+     |
|   - EP0: Descriptors (Logitech Mouse) |                      |                   |                   |
|   - EP1: Interrupt IN Reports (4-byte)|                      |                   v                   |
|                                       |                      |         "HID-compliant mouse"         |
|                                       |                      |        (Genuine Physical Mouse)       |
+---------------------------------------+                      +---------------------------------------+
```

---

## Quick Start Guide

### Step 1: Install the USB/IP Driver on the Host PC (One-Time Setup)

On the **Host PC** (the machine that will receive the mouse input), install the signed Windows USB/IP driver:

```powershell
winget install vadimgrn.usbip-win2
```

*(Note: If you run `VirtualMouseHost.exe` without the driver, it will detect it and offer to install it for you automatically).*

---

### Step 2: Start the Server on the Server PC

On the **Server PC** (the machine whose mouse you want to map), double-click:

```cmd
VirtualMouseServer.exe
```

*(Or run `run_server.bat`)*

Options:
```text
Usage: VirtualMouseServer.exe [OPTIONS]
Options:
  --host <IP>      Bind address (default: 0.0.0.0)
  --port <PORT>    Bind port (default: 3240)
  --delay <SECS>   Countdown delay in seconds (default: 5)
```

---

### Step 3: Connect on the Host PC

On the **Host PC**, double-click:

```cmd
VirtualMouseHost.exe
```

*(Or run `attach.bat`)*

1. It will prompt for the Server PC's IP address (press Enter for `127.0.0.1` or type the remote IP).
2. It attaches the USB mouse (`busid 1-1`) to Windows.
3. Windows Device Manager registers a new **"HID-compliant mouse"**.
4. Pressing `[Enter]` or `Ctrl+C` in `VirtualMouseHost.exe` cleanly detaches the USB mouse before exiting.

---

### Step 4: Capture & Control

1. Once the Host PC is attached, you will see `[USB/IP] Host attached successfully` in the server console.
2. Press **`F12`** on the Server PC.
3. You will hear 5 countdown beeps (1 per second). Use this time to switch into and focus your full-screen Remote Desktop window.
4. On the final high chime, mouse capture activates! Your physical mouse movements stream directly to the Host PC.
5. Press **`F12`** again anytime to immediately release mouse control back to the Server PC.

---

## Building from Source

Requirements:
- Windows 10 / 11 (x64)
- Rust toolchain (`rustup install stable`)

To build both executables:
```cmd
build.bat
```
*(Or run `cargo build --release`)*

The compiled binaries will be placed in root and `dist/`:
- `VirtualMouseServer.exe`
- `VirtualMouseHost.exe`

---

## Running Automated Tests

To run the end-to-end integration test:

```cmd
cargo test
```

---

## Project Structure

```text
VirtualMouse/
├── src/
│   ├── main.rs          # VirtualMouseServer: Raw Input capture, USB/IP server, audio chimes
│   ├── host_main.rs     # VirtualMouseHost: Driver detection, device attachment, clean detachment
│   ├── capture.rs       # Win32 Raw Input capture, global hotkey poller (F12/F11/ScrollLock)
│   ├── descriptors.rs   # USB 2.0 HID mouse descriptors (Logitech 0x046D:0xC077)
│   └── usbip.rs         # Native USB/IP protocol server (devlist, import, URB protocol loop)
├── tests/
│   └── test_server.rs   # Native Rust integration test (cargo test)
├── VirtualMouseServer.exe # Standalone Server executable
├── VirtualMouseHost.exe   # Standalone Host executable
├── attach.bat           # 1-click launcher for VirtualMouseHost.exe
├── detach.bat           # 1-click launcher to detach
├── run_server.bat       # 1-click launcher for VirtualMouseServer.exe
├── build.bat            # 1-click compile script for both executables
├── Cargo.toml           # Rust dual-binary package configuration
└── README.md            # Complete documentation
```
