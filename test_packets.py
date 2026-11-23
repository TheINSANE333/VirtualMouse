import struct
import unittest

BASIC_HEADER_FORMAT = ">IIIII"
CMD_SUBMIT_FORMAT = ">IIiii8s"
RET_SUBMIT_FORMAT = ">iiiii8s"

USBIP_CMD_SUBMIT = 0x00000001
USBIP_RET_SUBMIT = 0x00000003

class TestUsbipPackets(unittest.TestCase):
    def test_header_sizes(self):
        self.assertEqual(struct.calcsize(BASIC_HEADER_FORMAT), 20)
        self.assertEqual(struct.calcsize(CMD_SUBMIT_FORMAT), 28)
        self.assertEqual(struct.calcsize(RET_SUBMIT_FORMAT), 28)
        self.assertEqual(20 + 28, 48)

    def test_ret_submit_packing(self):
        seqnum = 42
        status = 0
        data = b"\x01\x02\x03\x04"
        actual_length = len(data)

        header = struct.pack(BASIC_HEADER_FORMAT, USBIP_RET_SUBMIT, seqnum, 0, 0, 0)
        body = struct.pack(RET_SUBMIT_FORMAT, status, actual_length, 0, 0, 0, b"\x00" * 8)
        packet = header + body + data

        self.assertEqual(len(packet), 48 + 4)
        cmd, ret_seq, _, _, _ = struct.unpack_from(BASIC_HEADER_FORMAT, packet, 0)
        self.assertEqual(cmd, USBIP_RET_SUBMIT)
        self.assertEqual(ret_seq, 42)

if __name__ == "__main__":
    unittest.main()
