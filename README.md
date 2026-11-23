# VirtualMouse: Minimal USB-over-IP Mouse

A minimalist, open-source alternative to **VirtualHere** designed to map mouse input from another computer (or virtual source) to a Windows Host PC as a **genuine physical USB mouse** (`HID-compliant mouse`).

---

## Why VirtualHere Works & Why This Works

In Windows, injecting mouse events via `SendInput` or software sharing tools (like Barrier / Synergy / Mouse Without Borders) only generates **synthetic software events** flagged with `LLMHF_INJECTED`. Many games, anticheat systems, Raw Input (`WM_INPUT`) applications, and secure desktops completely ignore or block synthetic mouse inputs.

**VirtualHere** solves this by forwarding raw USB packets over IP to a **Virtual Host Controller Driver (VHCI / UDE)** on the Host PC. When attached, the Windows kernel loads standard Microsoft USB and HID drivers (`usbhub.sys` -> `hidusb.sys` -> `mouhid.sys`). Windows Device Manager literally sees a real physical mouse plugged into a USB port.

This project implements that exact architecture in a **minimal, lightweight, zero-dependency Python stack** using the open **USB/IP** protocol.

```
+--------------------------+                         +--------------------------+
|        Sender PC         |                         |      Host PC (Windows)   |
|  (Mouse capture / test)  |                         |                          |
|                          |     TCP Port 3240       |  +--------------------+  |
|      server.py           | ----------------------> |  | usbip-win2 Driver  |  |
|  (USB/IP Virtual Mouse)  |       (USB/IP)          |  +--------------------+  |
|                          |                         |           | (Kernel USB) |
|   - EP0: Descriptors     |                         |           v              |
|   - EP1: Mouse Reports   |                         |   "HID-compliant mouse"  |
+--------------------------+                         |  (Raw Input / Physical)  |
                                                     +--------------------------+
```

---

## Features

- **True Physical USB Mouse**: Appears in Windows Device Manager with real Vendor ID (`0x046D`) and Product ID (`0xC077`). Full Raw Input support.
- **Ultra-Minimal Codebase**: The entire USB/IP server is contained in a single self-contained Python file (`server.py`) using only the standard library.
- **Interactive Capture Window**: Built-in Tkinter capture window to forward mouse movements, clicks, and scroll wheel.
- **Automated Test Mode**: `--test` generates smooth circular mouse movements to verify end-to-end operation without needing a second PC.
- **One-Command Driver Setup**: Uses the signed, open-source `usbip-win2` driver via `winget`.

---

## Quick Start Guide

### Step 1: Install the USB/IP Driver on the Host PC (One-Time Setup)

On the **Host PC (Windows)**, open PowerShell as Administrator and run:

```powershell
winget install vadimgrn.usbip-win2
```

*(Or download the installer from [vadimgrn/usbip-win2 releases](https://github.com/vadimgrn/usbip-win2/releases).)*

---

### Step 2: Start the Server on the Sender PC

On the **Sender PC** (or secondary machine), run:

```bash
python server.py
```

This launches a small capture window.
- **Click inside the box** to capture mouse control.
- Relative mouse deltas, button presses (Left/Right/Middle), and wheel scrolls are streamed to the Host PC.
- Press **`ESC`** at any time to release mouse capture.

#### Additional Modes:
- **Test Mode** (Generates continuous circular mouse movements to test connection):
  ```bash
  python server.py --test
  ```
- **Headless Mode** (Runs in background without GUI):
  ```bash
  python server.py --headless
  ```

---

### Step 3: Attach the Mouse on the Host PC

On the **Host PC**, run:

```bash
python client.py <SENDER_PC_IP>
```
*(Example: `python client.py 192.168.1.100`)*

Or double-click `attach.bat` and enter the IP address.

#### What Happens:
1. The client queries the Sender PC via USB/IP.
2. The virtual mouse (`busid: 1-1`) is attached to Windows.
3. Windows Device Manager registers a new **"HID-compliant mouse"**.
4. The cursor moves natively.
5. Press `Ctrl+C` in the terminal to cleanly detach the virtual USB port.

---

## Running Automated Tests

To verify the USB/IP handshake, descriptor enumeration, and interrupt report streaming on loopback (`127.0.0.1`):

```bash
python test_mock_client.py
```

---

## Alternative: Hardware USB Dongle (Zero-Driver Solution)

If your Host PC environment strictly forbids installing third-party drivers (e.g. corporate managed laptops or strict tournament anticheat rules):

You can use a **\$4 Raspberry Pi Pico** or **Arduino Leonardo / Pro Micro (ATmega32U4)** plugged into the Host PC's USB port:
1. The microcontroller runs standard USB HID Mouse firmware (recognized 100% as native hardware by Windows with no drivers needed).
2. The Sender PC connects to the microcontroller via Serial (USB COM port), Bluetooth, or UDP (Pico W).
3. The microcontroller converts received coordinates into standard USB mouse packets.
