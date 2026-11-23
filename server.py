"""
VirtualMouse - Minimal USB-over-IP Mouse Server
Emulates a real physical USB HID Mouse over the standard USB/IP protocol (TCP port 3240).
"""

import sys
import socket
import struct
import threading
import time
import queue
import argparse

# ==========================================
# USB/IP Protocol Constants
# ==========================================
USBIP_PORT = 3240
USBIP_VERSION = 0x0111  # USB/IP protocol v1.1.1

# Management opcodes
OP_REQ_DEVLIST = 0x8005
OP_REP_DEVLIST = 0x0005
OP_REQ_IMPORT  = 0x8003
OP_REP_IMPORT  = 0x0003

# URB transfer commands
USBIP_CMD_SUBMIT = 0x00000001
USBIP_RET_SUBMIT = 0x00000003
USBIP_CMD_UNLINK = 0x00000002
USBIP_RET_UNLINK = 0x00000004

# Direction flags
USBIP_DIR_OUT = 0
USBIP_DIR_IN  = 1

# ==========================================
# USB Hardware Emulation Descriptors
# ==========================================
# Standard 5-Button Mouse HID Report Descriptor with Wheel (52 bytes)
HID_REPORT_DESC = bytes([
    0x05, 0x01,        # USAGE_PAGE (Generic Desktop)
    0x09, 0x02,        # USAGE (Mouse)
    0xA1, 0x01,        # COLLECTION (Application)
    0x09, 0x01,        #   USAGE (Pointer)
    0xA1, 0x00,        #   COLLECTION (Physical)
    # 5 Buttons: Left, Right, Middle, Side 1, Side 2
    0x05, 0x09,        #     USAGE_PAGE (Button)
    0x19, 0x01,        #     USAGE_MINIMUM (Button 1)
    0x29, 0x05,        #     USAGE_MAXIMUM (Button 5)
    0x15, 0x00,        #     LOGICAL_MINIMUM (0)
    0x25, 0x01,        #     LOGICAL_MAXIMUM (1)
    0x95, 0x05,        #     REPORT_COUNT (5)
    0x75, 0x01,        #     REPORT_SIZE (1)
    0x81, 0x02,        #     INPUT (Data, Var, Abs)
    # 3 bits padding
    0x95, 0x01,        #     REPORT_COUNT (1)
    0x75, 0x03,        #     REPORT_SIZE (3)
    0x81, 0x01,        #     INPUT (Cnst, Ary, Abs)
    # X and Y Relative Movement (-127 to 127)
    0x05, 0x01,        #     USAGE_PAGE (Generic Desktop)
    0x09, 0x30,        #     USAGE (X)
    0x09, 0x31,        #     USAGE (Y)
    0x15, 0x81,        #     LOGICAL_MINIMUM (-127)
    0x25, 0x7F,        #     LOGICAL_MAXIMUM (127)
    0x75, 0x08,        #     REPORT_SIZE (8)
    0x95, 0x02,        #     REPORT_COUNT (2)
    0x81, 0x06,        #     INPUT (Data, Var, Rel)
    # Wheel Movement (-127 to 127)
    0x09, 0x38,        #     USAGE (Wheel)
    0x15, 0x81,        #     LOGICAL_MINIMUM (-127)
    0x25, 0x7F,        #     LOGICAL_MAXIMUM (127)
    0x75, 0x08,        #     REPORT_SIZE (8)
    0x95, 0x01,        #     REPORT_COUNT (1)
    0x81, 0x06,        #     INPUT (Data, Var, Rel)
    0xC0,              #   END_COLLECTION
    0xC0               # END_COLLECTION
])

# USB Device Descriptor (18 bytes)
DEV_DESC = struct.pack(
    "<BBHBBBBHHHBBBB",
    18,          # bLength
    0x01,        # bDescriptorType (Device)
    0x0200,      # bcdUSB (2.0)
    0x00,        # bDeviceClass (Specified at interface level)
    0x00,        # bDeviceSubClass
    0x00,        # bDeviceProtocol
    64,          # bMaxPacketSize0
    0x046D,      # idVendor (Logitech)
    0xC077,      # idProduct (Optical USB Mouse)
    0x0100,      # bcdDevice
    1,           # iManufacturer
    2,           # iProduct
    3,           # iSerialNumber
    1            # bNumConfigurations
)

# Configuration Descriptor (34 bytes total)
CFG_DESC = struct.pack(
    "<BBHBBBBB",
    9,           # bLength
    0x02,        # bDescriptorType (Configuration)
    34,          # wTotalLength (9 + 9 + 9 + 7)
    1,           # bNumInterfaces
    1,           # bConfigurationValue
    0,           # iConfiguration
    0xA0,        # bmAttributes (Bus powered, remote wakeup)
    50           # bMaxPower (100 mA)
)

IF_DESC = struct.pack(
    "<BBBBBBBBB",
    9,           # bLength
    0x04,        # bDescriptorType (Interface)
    0,           # bInterfaceNumber
    0,           # bAlternateSetting
    1,           # bNumEndpoints
    0x03,        # bInterfaceClass (HID)
    0x01,        # bInterfaceSubClass (Boot Interface)
    0x02,        # bInterfaceProtocol (Mouse)
    0            # iInterface
)

HID_DESC = struct.pack(
    "<BBHBBBH",
    9,           # bLength
    0x21,        # bDescriptorType (HID)
    0x0111,      # bcdHID (HID 1.11)
    0x00,        # bCountryCode
    1,           # bNumDescriptors
    0x22,        # bDescriptorType (Report)
    len(HID_REPORT_DESC)
)

EP_DESC = struct.pack(
    "<BBBBHB",
    7,           # bLength
    0x05,        # bDescriptorType (Endpoint)
    0x81,        # bEndpointAddress (EP 1 IN)
    0x03,        # bmAttributes (Interrupt)
    64,          # wMaxPacketSize
    1            # bInterval (1ms polling)
)

FULL_CONFIG_DESC = CFG_DESC + IF_DESC + HID_DESC + EP_DESC

def make_string_desc(s: str) -> bytes:
    encoded = s.encode("utf-16le")
    return bytes([len(encoded) + 2, 0x03]) + encoded

STR_DESCS = {
    0: bytes([4, 3, 0x09, 0x04]),       # English (US)
    1: make_string_desc("VirtualUSB"),
    2: make_string_desc("Physical Optical Mouse"),
    3: make_string_desc("VMOUSE0001"),
}

# ==========================================
# USB/IP Device Structure Packing
# ==========================================
def pack_device_info(busid=b"1-1", path=b"/sys/devices/virtual/usb/1-1") -> bytes:
    """Packs the 312-byte usbip_usb_device structure."""
    return struct.pack(
        ">256s32sIIIHHHBBBBBB",
        path.ljust(256, b"\x00"),
        busid.ljust(32, b"\x00"),
        1,          # busnum
        1,          # devnum
        2,          # speed: USB_SPEED_FULL (12 Mbps)
        0x046D,     # idVendor
        0xC077,     # idProduct
        0x0100,     # bcdDevice
        0x00,       # bDeviceClass
        0x00,       # bDeviceSubClass
        0x00,       # bDeviceProtocol
        0x01,       # bConfigurationValue
        0x01,       # bNumConfigurations
        0x01        # bNumInterfaces
    )

def pack_interface_info() -> bytes:
    """Packs the 4-byte usbip_usb_interface structure."""
    return struct.pack(
        ">BBBB",
        0x03,       # bInterfaceClass (HID)
        0x01,       # bInterfaceSubClass (Boot)
        0x02,       # bInterfaceProtocol (Mouse)
        0x00        # padding
    )


class VirtualMouseServer:
    def __init__(self, host="0.0.0.0", port=USBIP_PORT):
        self.host = host
        self.port = port
        self.running = False
        self.mouse_queue = queue.Queue(maxsize=1000)
        self.active_client_sock = None
        self.lock = threading.Lock()
        self.buttons_state = 0  # Bitmask: bit0=L, bit1=R, bit2=M

    def push_mouse_event(self, buttons: int, dx: int, dy: int, wheel: int = 0):
        """Pushes a mouse report (buttons, dx, dy, wheel)."""
        # Clamp coordinates to signed int8 (-127 to 127)
        dx = max(-127, min(127, int(dx)))
        dy = max(-127, min(127, int(dy)))
        wheel = max(-127, min(127, int(wheel)))
        btn = int(buttons) & 0x1F

        # Format 4-byte HID report
        report = struct.pack("<Bbbb", btn, dx, dy, wheel)
        try:
            self.mouse_queue.put_nowait(report)
        except queue.Full:
            try:
                self.mouse_queue.get_nowait()
            except queue.Empty:
                pass
            self.mouse_queue.put_nowait(report)

    def handle_devlist(self, conn):
        """Handles OP_REQ_DEVLIST by advertising 1 virtual mouse device."""
        print("[USB/IP] Client requested device list (OP_REQ_DEVLIST)")
        header = struct.pack(">HHII", USBIP_VERSION, OP_REP_DEVLIST, 0, 1)
        dev_info = pack_device_info()
        if_info = pack_interface_info()
        conn.sendall(header + dev_info + if_info)

    def handle_import(self, conn, busid_raw):
        """Handles OP_REQ_IMPORT to attach the virtual mouse."""
        busid = busid_raw.rstrip(b"\x00").decode("ascii", errors="ignore")
        print(f"[USB/IP] Client importing device busid: '{busid}'")
        if busid == "1-1":
            header = struct.pack(">HHI", USBIP_VERSION, OP_REP_IMPORT, 0)
            dev_info = pack_device_info()
            conn.sendall(header + dev_info)
            print("[USB/IP] Device imported successfully! Switching to URB stream.")
            self.run_urb_loop(conn)
        else:
            print(f"[USB/IP] Unknown busid: {busid}, rejecting.")
            header = struct.pack(">HHI", USBIP_VERSION, OP_REP_IMPORT, 1)
            conn.sendall(header)

    def run_urb_loop(self, conn):
        """Processes USB Request Blocks (URBs) from the Host PC."""
        conn.settimeout(None)
        with self.lock:
            self.active_client_sock = conn

        try:
            while self.running:
                # Read 48-byte USBIP header
                header_data = self._recv_exact(conn, 48)
                if not header_data:
                    break

                # Parse Basic Header (20 bytes)
                cmd, seqnum, devid, direction, ep = struct.unpack(">IIIII", header_data[:20])

                if cmd == USBIP_CMD_SUBMIT:
                    # Parse Cmd Submit Body (28 bytes)
                    flags, length, start_frame, num_packets, interval, setup = struct.unpack(
                        ">IIiii8s", header_data[20:48]
                    )

                    # Read payload for OUT transfers if any
                    out_payload = b""
                    if direction == USBIP_DIR_OUT and length > 0:
                        out_payload = self._recv_exact(conn, length)

                    if ep == 0:
                        # Control Endpoint 0
                        self._handle_control_urb(conn, seqnum, direction, length, setup, out_payload)
                    elif ep == 1 and direction == USBIP_DIR_IN:
                        # Interrupt IN Endpoint 1 (Mouse Report)
                        self._handle_interrupt_urb(conn, seqnum, length)
                    else:
                        # Unknown endpoint - return 0 bytes success
                        self._send_ret_submit(conn, seqnum, 0, b"")

                elif cmd == USBIP_CMD_UNLINK:
                    unlink_seqnum = struct.unpack(">I", header_data[20:24])[0]
                    # Reply to unlink
                    ret_unlink = struct.pack(">IIIIIi24s", USBIP_RET_UNLINK, seqnum, 0, 0, 0, 0, b"\x00" * 24)
                    conn.sendall(ret_unlink)

        except (socket.error, ConnectionResetError, BrokenPipeError) as e:
            print(f"[USB/IP] Client disconnected: {e}")
        finally:
            with self.lock:
                self.active_client_sock = None

    def _handle_control_urb(self, conn, seqnum, direction, length, setup, out_payload):
        """Handles USB Standard & HID Control transfers on EP0."""
        bmRequestType, bRequest, wValue, wIndex, wLength = struct.unpack("<BBHHH", setup)
        req_type = (bmRequestType >> 5) & 0x03

        payload = b""

        if req_type == 0:  # Standard USB Request
            if bRequest == 0x06:  # GET_DESCRIPTOR
                desc_type = (wValue >> 8) & 0xFF
                desc_index = wValue & 0xFF
                if desc_type == 1:       # Device Descriptor
                    payload = DEV_DESC
                elif desc_type == 2:     # Configuration Descriptor
                    payload = FULL_CONFIG_DESC
                elif desc_type == 3:     # String Descriptor
                    payload = STR_DESCS.get(desc_index, b"")
                elif desc_type == 0x22:  # HID Report Descriptor
                    payload = HID_REPORT_DESC
            elif bRequest == 0x09:  # SET_CONFIGURATION
                payload = b""
            elif bRequest == 0x00:  # GET_STATUS
                payload = b"\x00\x00"
        elif req_type == 1:  # HID Class-Specific Request
            if bRequest == 0x0A:  # SET_IDLE
                payload = b""
            elif bRequest == 0x01:  # GET_REPORT
                payload = b"\x00\x00\x00\x00"
            elif bRequest == 0x09:  # SET_REPORT
                payload = b""

        # Trim payload to requested length
        if length > 0 and len(payload) > length:
            payload = payload[:length]

        self._send_ret_submit(conn, seqnum, 0, payload)

    def _handle_interrupt_urb(self, conn, seqnum, length):
        """Handles polling from Host for mouse reports."""
        try:
            # Wait up to 10ms for a mouse movement/click event
            report = self.mouse_queue.get(timeout=0.01)
        except queue.Empty:
            # Real USB HID mice NAK when there's no new motion;
            # in USB/IP, returning an empty report or 0s preserves keepalive
            report = b"\x00\x00\x00\x00"

        if length > 0 and len(report) > length:
            report = report[:length]

        self._send_ret_submit(conn, seqnum, 0, report)

    def _send_ret_submit(self, conn, seqnum, status, payload):
        """Packs and sends USBIP_RET_SUBMIT."""
        header = struct.pack(">IIIII", USBIP_RET_SUBMIT, seqnum, 0, 0, 0)
        body = struct.pack(">iiiii8s", status, len(payload), 0, 0, 0, b"\x00" * 8)
        conn.sendall(header + body + payload)

    def _recv_exact(self, sock, num_bytes):
        """Receives exactly num_bytes from socket."""
        buf = bytearray()
        while len(buf) < num_bytes:
            chunk = sock.recv(num_bytes - len(buf))
            if not chunk:
                return None
            buf.extend(chunk)
        return bytes(buf)

    def start(self):
        """Starts the USB/IP server socket listener."""
        self.running = True
        server_sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        server_sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        server_sock.bind((self.host, self.port))
        server_sock.listen(5)
        print(f"[USB/IP Server] Listening on {self.host}:{self.port}...")

        try:
            while self.running:
                conn, addr = server_sock.accept()
                print(f"[USB/IP] Connection accepted from {addr[0]}:{addr[1]}")
                # Read initial 8-byte management header
                header = self._recv_exact(conn, 8)
                if not header:
                    conn.close()
                    continue

                version, opcode, status = struct.unpack(">HHI", header)
                if opcode == OP_REQ_DEVLIST:
                    self.handle_devlist(conn)
                    conn.close()
                elif opcode == OP_REQ_IMPORT:
                    busid_raw = self._recv_exact(conn, 32)
                    if busid_raw:
                        # Run import in separate worker thread
                        t = threading.Thread(target=self.handle_import, args=(conn, busid_raw), daemon=True)
                        t.start()
                    else:
                        conn.close()
                else:
                    print(f"[USB/IP] Unknown management opcode: 0x{opcode:04X}")
                    conn.close()
        except KeyboardInterrupt:
            print("\n[USB/IP Server] Stopping...")
        finally:
            self.running = False
            server_sock.close()


# ==========================================
# Input Capture & Test Generator Modes
# ==========================================
def run_test_motion(server: VirtualMouseServer):
    """Generates continuous smooth circular mouse movement for testing."""
    import math
    print("[Test Mode] Generating continuous circular mouse movement...")
    angle = 0.0
    radius = 5.0
    while server.running:
        dx = int(math.cos(angle) * radius)
        dy = int(math.sin(angle) * radius)
        server.push_mouse_event(buttons=0, dx=dx, dy=dy, wheel=0)
        angle += 0.1
        time.sleep(0.016)  # ~60 Hz update rate


def run_tkinter_capture(server: VirtualMouseServer):
    """
    Lightweight, zero-dependency GUI capture window.
    Focus the window to capture mouse deltas and send to Host PC.
    """
    import tkinter as tk
    root = tk.Tk()
    root.title("VirtualMouse - Capture Window")
    root.geometry("320x200")
    root.attributes("-topmost", True)

    label = tk.Label(
        root,
        text="Click inside this box to map mouse\nto the Host PC.\n\nPress ESC to release capture.",
        font=("Segoe UI", 11),
        justify="center",
        bg="#1e1e1e",
        fg="#ffffff"
    )
    label.pack(fill="both", expand=True, padx=10, pady=10)

    last_pos = [None, None]
    captured = [False]

    def on_click(event):
        captured[0] = True
        last_pos[0] = event.x_root
        last_pos[1] = event.y_root
        root.config(cursor="none")
        label.config(text="CAPTURED!\nMoving mouse forwards to Host PC.\n\nPress ESC to STOP.", fg="#4caf50")

    def on_escape(event):
        captured[0] = False
        last_pos[0] = None
        last_pos[1] = None
        root.config(cursor="")
        label.config(text="PAUSED\nClick inside to capture mouse again.\n\nPress ESC to release.", fg="#ffffff")

    def on_motion(event):
        if not captured[0]:
            return
        if last_pos[0] is not None and last_pos[1] is not None:
            dx = event.x_root - last_pos[0]
            dy = event.y_root - last_pos[1]
            if dx != 0 or dy != 0:
                server.push_mouse_event(buttons=server.buttons_state, dx=dx, dy=dy)
        last_pos[0] = event.x_root
        last_pos[1] = event.y_root

    def on_button_down(event, btn_mask):
        if captured[0]:
            server.buttons_state |= btn_mask
            server.push_mouse_event(buttons=server.buttons_state, dx=0, dy=0)

    def on_button_up(event, btn_mask):
        if captured[0]:
            server.buttons_state &= ~btn_mask
            server.push_mouse_event(buttons=server.buttons_state, dx=0, dy=0)

    def on_wheel(event):
        if captured[0]:
            # delta is usually +/- 120
            w = 1 if event.delta > 0 else -1
            server.push_mouse_event(buttons=server.buttons_state, dx=0, dy=0, wheel=w)

    label.bind("<Button-1>", on_click)
    root.bind("<Escape>", on_escape)
    root.bind("<Motion>", on_motion)
    root.bind("<ButtonPress-1>", lambda e: on_button_down(e, 1))
    root.bind("<ButtonRelease-1>", lambda e: on_button_up(e, 1))
    root.bind("<ButtonPress-3>", lambda e: on_button_down(e, 2))
    root.bind("<ButtonRelease-3>", lambda e: on_button_up(e, 2))
    root.bind("<ButtonPress-2>", lambda e: on_button_down(e, 4))
    root.bind("<ButtonRelease-2>", lambda e: on_button_up(e, 4))
    root.bind("<MouseWheel>", on_wheel)

    try:
        root.mainloop()
    finally:
        server.running = False


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Minimal USB-over-IP Mouse Server")
    parser.add_argument("--host", default="0.0.0.0", help="Listen IP (default 0.0.0.0)")
    parser.add_argument("--port", type=int, default=USBIP_PORT, help="Listen port (default 3240)")
    parser.add_argument("--test", action="store_true", help="Run in test mode (generates circle movement)")
    parser.add_argument("--headless", action="store_true", help="Run without capture window")
    args = parser.parse_args()

    v_server = VirtualMouseServer(host=args.host, port=args.port)

    # Server socket thread
    server_thread = threading.Thread(target=v_server.start, daemon=True)
    server_thread.start()
    time.sleep(0.2)

    if args.test:
        run_test_motion(v_server)
    elif args.headless:
        print("[VirtualMouse] Running headless. Press Ctrl+C to stop.")
        try:
            while True:
                time.sleep(1)
        except KeyboardInterrupt:
            pass
    else:
        # Launch capture GUI
        run_tkinter_capture(v_server)
