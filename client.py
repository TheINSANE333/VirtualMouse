"""
VirtualMouse - Host PC Client Controller
Connects the remote mouse to the Windows Host PC as a genuine physical USB mouse.
Uses the open-source USB/IP Windows UDE / VHCI client driver.
"""

import sys
import os
import subprocess
import shutil
import time
import argparse
import re

def check_usbip_installed() -> str:
    """Checks if usbip.exe is available in PATH or Program Files."""
    # Check PATH first
    path = shutil.which("usbip")
    if path:
        return path

    # Check common install locations for usbip-win2
    candidates = [
        r"C:\Program Files\usbip\usbip.exe",
        r"C:\Program Files\usbip-win2\usbip.exe",
        os.path.expandvars(r"%LOCALAPPDATA%\Programs\usbip\usbip.exe"),
    ]
    for c in candidates:
        if os.path.isfile(c):
            return c

    return None


def prompt_install_driver():
    """Provides 1-command instruction to install usbip-win2 via winget."""
    print("=" * 65)
    print(" [!] USB/IP Driver not found on this Host PC.")
    print(" Windows requires the USB/IP VHCI driver so that remote inputs")
    print(" are registered as an authentic physical USB mouse in Device Manager.")
    print("=" * 65)
    print("\nTo install it automatically using Windows Package Manager, run:")
    print("    winget install vadimgrn.usbip-win2\n")
    print("Or download the signed installer manually from:")
    print("    https://github.com/vadimgrn/usbip-win2/releases\n")


def list_remote_devices(usbip_bin: str, server_ip: str):
    """Lists available USB devices on the server."""
    print(f"[*] Querying {server_ip} for shared USB devices...")
    try:
        res = subprocess.run([usbip_bin, "list", "-r", server_ip], capture_output=True, text=True, timeout=5)
        print(res.stdout)
        if res.returncode != 0:
            print(f"[!] Error querying server: {res.stderr}")
            return False
        return True
    except subprocess.TimeoutExpired:
        print(f"[!] Timed out connecting to {server_ip}:3240. Check firewall and IP address.")
        return False
    except Exception as e:
        print(f"[!] Failed to list devices: {e}")
        return False


def get_imported_port(usbip_bin: str, busid: str = "1-1") -> str:
    """Finds the local virtual port assigned to the imported device."""
    try:
        res = subprocess.run([usbip_bin, "port"], capture_output=True, text=True, timeout=5)
        for line in res.stdout.splitlines():
            # Look for lines like "Port 00: <Port in Use> at Full Speed(12Mbps)" or containing busid
            match = re.search(r"Port\s+(\d+):", line, re.IGNORECASE)
            if match and ("1-1" in res.stdout or "046d:c077" in res.stdout):
                return match.group(1)
    except Exception:
        pass
    return None


def attach_device(usbip_bin: str, server_ip: str, busid: str = "1-1"):
    """Attaches the remote USB mouse to Windows."""
    print(f"[*] Attaching remote mouse from {server_ip} (busid: {busid})...")
    try:
        res = subprocess.run([usbip_bin, "attach", "-r", server_ip, "-b", busid], capture_output=True, text=True)
        if res.returncode == 0:
            print("[+] Successfully attached! Windows is now registering the physical mouse.")
            return True
        else:
            print(f"[!] Attach failed: {res.stderr or res.stdout}")
            return False
    except Exception as e:
        print(f"[!] Error running attach: {e}")
        return False


def detach_device(usbip_bin: str, port: str = None):
    """Detaches the remote USB device from Windows."""
    if port is None:
        port = get_imported_port(usbip_bin)

    if port:
        print(f"[*] Detaching virtual port {port}...")
        subprocess.run([usbip_bin, "detach", "-p", str(port)], capture_output=True)
        print("[+] Detached successfully.")
    else:
        # Try port 00 or 0
        subprocess.run([usbip_bin, "detach", "-p", "0"], capture_output=True)
        subprocess.run([usbip_bin, "detach", "-p", "00"], capture_output=True)


def main():
    parser = argparse.ArgumentParser(description="Host PC Connector for VirtualMouse")
    parser.add_argument("server_ip", nargs="?", default="127.0.0.1", help="IP address of the Sender PC")
    parser.add_argument("--busid", default="1-1", help="Device Bus ID (default 1-1)")
    parser.add_argument("--detach", action="store_true", help="Detach any currently attached virtual mouse")
    args = parser.parse_args()

    usbip_bin = check_usbip_installed()
    if not usbip_bin:
        prompt_install_driver()
        sys.exit(1)

    print(f"[+] Found USB/IP binary: {usbip_bin}")

    if args.detach:
        detach_device(usbip_bin)
        return

    # 1. Discover device
    if not list_remote_devices(usbip_bin, args.server_ip):
        sys.exit(1)

    # 2. Attach device
    if not attach_device(usbip_bin, args.server_ip, args.busid):
        sys.exit(1)

    print("=" * 65)
    print(" [SUCCESS] Remote mouse mapped as PHYSICAL MOUSE on Host PC!")
    print(" - Visible in Windows Device Manager -> Mice and other pointing devices")
    print(" - Recognized as a hardware USB HID device by Windows Raw Input")
    print(" - Press Ctrl+C in this terminal to disconnect and detach cleanly.")
    print("=" * 65)

    try:
        while True:
            time.sleep(1)
    except KeyboardInterrupt:
        print("\n[*] Disconnecting...")
    finally:
        detach_device(usbip_bin)


if __name__ == "__main__":
    main()
