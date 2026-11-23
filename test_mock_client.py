"""
Test Suite for VirtualMouse Server
Verifies USB/IP protocol handshake, USB descriptors, and Interrupt IN mouse reports.
"""

import socket
import struct
import threading
import time
import unittest
from server import (
    VirtualMouseServer,
    USBIP_VERSION,
    OP_REQ_DEVLIST,
    OP_REP_DEVLIST,
    OP_REQ_IMPORT,
    OP_REP_IMPORT,
    USBIP_CMD_SUBMIT,
    USBIP_RET_SUBMIT,
    DEV_DESC,
    FULL_CONFIG_DESC,
    HID_REPORT_DESC,
)

TEST_PORT = 13240

class TestVirtualMouseServer(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = VirtualMouseServer(host="127.0.0.1", port=TEST_PORT)
        cls.server_thread = threading.Thread(target=cls.server.start, daemon=True)
        cls.server_thread.start()
        time.sleep(0.3)

    def test_01_devlist(self):
        """Tests device enumeration (OP_REQ_DEVLIST)."""
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.connect(("127.0.0.1", TEST_PORT))

        # Send OP_REQ_DEVLIST (8 bytes)
        req = struct.pack(">HHI", USBIP_VERSION, OP_REQ_DEVLIST, 0)
        s.sendall(req)

        # Read 12-byte reply header
        rep_header = s.recv(12)
        version, opcode, status, ndev = struct.unpack(">HHII", rep_header)

        self.assertEqual(version, USBIP_VERSION)
        self.assertEqual(opcode, OP_REP_DEVLIST)
        self.assertEqual(status, 0)
        self.assertEqual(ndev, 1)

        # Read 312-byte usbip_usb_device
        dev_bytes = s.recv(312)
        self.assertEqual(len(dev_bytes), 312)

        # Unpack device fields
        path, busid, busnum, devnum, speed, vid, pid, bcdDev, dClass, dSub, dProto, cVal, nCfg, nIf = struct.unpack(
            ">256s32sIIIHHHBBBBBB", dev_bytes
        )
        self.assertEqual(busid.rstrip(b"\x00"), b"1-1")
        self.assertEqual(vid, 0x046D)
        self.assertEqual(pid, 0xC077)
        self.assertEqual(nIf, 1)

        # Read 4-byte interface struct
        if_bytes = s.recv(4)
        self.assertEqual(len(if_bytes), 4)
        ifClass, ifSubClass, ifProto, _ = struct.unpack(">BBBB", if_bytes)
        self.assertEqual(ifClass, 0x03)      # HID
        self.assertEqual(ifSubClass, 0x01)   # Boot
        self.assertEqual(ifProto, 0x02)      # Mouse

        s.close()

    def test_02_import_and_urb_descriptors(self):
        """Tests OP_REQ_IMPORT and USB Descriptor URBs on EP0."""
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.connect(("127.0.0.1", TEST_PORT))

        # 1. Send OP_REQ_IMPORT (8 bytes + 32 bytes busid)
        busid_field = b"1-1".ljust(32, b"\x00")
        req = struct.pack(">HHI32s", USBIP_VERSION, OP_REQ_IMPORT, 0, busid_field)
        s.sendall(req)

        # Read 8-byte reply header
        rep_header = s.recv(8)
        version, opcode, status = struct.unpack(">HHI", rep_header)
        self.assertEqual(version, USBIP_VERSION)
        self.assertEqual(opcode, OP_REP_IMPORT)
        self.assertEqual(status, 0)

        # Read 312-byte dev struct
        dev_bytes = s.recv(312)
        self.assertEqual(len(dev_bytes), 312)

        # 2. Test GET_DESCRIPTOR for Device Descriptor
        # bmRequestType=0x80 (IN, Standard, Device), bRequest=0x06 (GET_DESC), wValue=0x0100 (Device 0), wIndex=0, wLength=18
        setup = struct.pack("<BBHHH", 0x80, 0x06, 0x0100, 0, 18)
        # CMD_SUBMIT: cmd(1), seqnum(101), devid(1), dir(1: IN), ep(0), flags(0), len(18), start(0), num(0), interval(0), setup(8s)
        cmd_hdr = struct.pack(">IIIII", USBIP_CMD_SUBMIT, 101, 1, 1, 0)
        cmd_body = struct.pack(">IIiii8s", 0, 18, 0, 0, 0, setup)
        s.sendall(cmd_hdr + cmd_body)

        # Read RET_SUBMIT (48 bytes + 18 bytes payload)
        ret_hdr = s.recv(48)
        ret_cmd, ret_seq, _, _, _ = struct.unpack(">IIIII", ret_hdr[:20])
        ret_status, ret_len, _, _, _, _ = struct.unpack(">iiiii8s", ret_hdr[20:48])

        self.assertEqual(ret_cmd, USBIP_RET_SUBMIT)
        self.assertEqual(ret_seq, 101)
        self.assertEqual(ret_status, 0)
        self.assertEqual(ret_len, 18)

        payload = s.recv(18)
        self.assertEqual(payload, DEV_DESC)

        # 3. Test GET_DESCRIPTOR for Configuration Descriptor
        setup_cfg = struct.pack("<BBHHH", 0x80, 0x06, 0x0200, 0, 34)
        cmd_hdr = struct.pack(">IIIII", USBIP_CMD_SUBMIT, 102, 1, 1, 0)
        cmd_body = struct.pack(">IIiii8s", 0, 34, 0, 0, 0, setup_cfg)
        s.sendall(cmd_hdr + cmd_body)

        ret_hdr = s.recv(48)
        ret_status, ret_len, _, _, _, _ = struct.unpack(">iiiii8s", ret_hdr[20:48])
        self.assertEqual(ret_status, 0)
        self.assertEqual(ret_len, 34)
        payload = s.recv(34)
        self.assertEqual(payload, FULL_CONFIG_DESC)

        # 4. Test GET_DESCRIPTOR for HID Report Descriptor
        setup_hid = struct.pack("<BBHHH", 0x81, 0x06, 0x2200, 0, len(HID_REPORT_DESC))
        cmd_hdr = struct.pack(">IIIII", USBIP_CMD_SUBMIT, 103, 1, 1, 0)
        cmd_body = struct.pack(">IIiii8s", 0, len(HID_REPORT_DESC), 0, 0, 0, setup_hid)
        s.sendall(cmd_hdr + cmd_body)

        ret_hdr = s.recv(48)
        ret_status, ret_len, _, _, _, _ = struct.unpack(">iiiii8s", ret_hdr[20:48])
        self.assertEqual(ret_status, 0)
        self.assertEqual(ret_len, len(HID_REPORT_DESC))
        payload = s.recv(len(HID_REPORT_DESC))
        self.assertEqual(payload, HID_REPORT_DESC)

        s.close()

    def test_03_mouse_interrupt_report(self):
        """Tests streaming mouse input reports on EP1."""
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.connect(("127.0.0.1", TEST_PORT))

        # Import device
        busid_field = b"1-1".ljust(32, b"\x00")
        req = struct.pack(">HHI32s", USBIP_VERSION, OP_REQ_IMPORT, 0, busid_field)
        s.sendall(req)
        s.recv(8 + 312)

        # Push a specific mouse event: Left click (1), dx=15, dy=-20, wheel=1
        self.server.push_mouse_event(buttons=1, dx=15, dy=-20, wheel=1)

        # Host polls EP1 Interrupt IN
        cmd_hdr = struct.pack(">IIIII", USBIP_CMD_SUBMIT, 201, 1, 1, 1)  # ep=1, dir=1 (IN)
        cmd_body = struct.pack(">IIiii8s", 0, 4, 0, 0, 0, b"\x00" * 8)
        s.sendall(cmd_hdr + cmd_body)

        # Read response
        ret_hdr = s.recv(48)
        ret_cmd, ret_seq, _, _, _ = struct.unpack(">IIIII", ret_hdr[:20])
        ret_status, ret_len, _, _, _, _ = struct.unpack(">iiiii8s", ret_hdr[20:48])

        self.assertEqual(ret_cmd, USBIP_RET_SUBMIT)
        self.assertEqual(ret_seq, 201)
        self.assertEqual(ret_status, 0)
        self.assertEqual(ret_len, 4)

        report = s.recv(4)
        btn, dx, dy, wheel = struct.unpack("<Bbbb", report)
        self.assertEqual(btn, 1)
        self.assertEqual(dx, 15)
        self.assertEqual(dy, -20)
        self.assertEqual(wheel, 1)

        s.close()

if __name__ == "__main__":
    unittest.main()
