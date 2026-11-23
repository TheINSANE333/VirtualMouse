// Standard USB HID Mouse Descriptors for VirtualMouse USB/IP Server

// 52-byte standard 5-button optical mouse report descriptor with relative X, Y, and wheel
pub const HID_REPORT_DESC: &[u8] = &[
    0x05, 0x01,        // USAGE_PAGE (Generic Desktop)
    0x09, 0x02,        // USAGE (Mouse)
    0xA1, 0x01,        // COLLECTION (Application)
    0x09, 0x01,        //   USAGE (Pointer)
    0xA1, 0x00,        //   COLLECTION (Physical)
    // 5 Buttons
    0x05, 0x09,        //     USAGE_PAGE (Button)
    0x19, 0x01,        //     USAGE_MINIMUM (Button 1)
    0x29, 0x05,        //     USAGE_MAXIMUM (Button 5)
    0x15, 0x00,        //     LOGICAL_MINIMUM (0)
    0x25, 0x01,        //     LOGICAL_MAXIMUM (1)
    0x95, 0x05,        //     REPORT_COUNT (5)
    0x75, 0x01,        //     REPORT_SIZE (1)
    0x81, 0x02,        //     INPUT (Data, Var, Abs)
    // 3 bits padding
    0x95, 0x01,        //     REPORT_COUNT (1)
    0x75, 0x03,        //     REPORT_SIZE (3)
    0x81, 0x01,        //     INPUT (Cnst, Ary, Abs)
    // X and Y Relative Movement (-127 to 127)
    0x05, 0x01,        //     USAGE_PAGE (Generic Desktop)
    0x09, 0x30,        //     USAGE (X)
    0x09, 0x31,        //     USAGE (Y)
    0x15, 0x81,        //     LOGICAL_MINIMUM (-127)
    0x25, 0x7F,        //     LOGICAL_MAXIMUM (127)
    0x75, 0x08,        //     REPORT_SIZE (8)
    0x95, 0x02,        //     REPORT_COUNT (2)
    0x81, 0x06,        //     INPUT (Data, Var, Rel)
    // Wheel Movement (-127 to 127)
    0x09, 0x38,        //     USAGE (Wheel)
    0x15, 0x81,        //     LOGICAL_MINIMUM (-127)
    0x25, 0x7F,        //     LOGICAL_MAXIMUM (127)
    0x75, 0x08,        //     REPORT_SIZE (8)
    0x95, 0x01,        //     REPORT_COUNT (1)
    0x81, 0x06,        //     INPUT (Data, Var, Rel)
    0xC0,              //   END_COLLECTION
    0xC0,              // END_COLLECTION
];

// Standard USB 2.0 Device Descriptor (18 bytes) - Logitech Optical Mouse (0x046D:0xC077)
pub const DEV_DESC: [u8; 18] = [
    18,          // bLength
    0x01,        // bDescriptorType (Device)
    0x00, 0x02,  // bcdUSB (2.0 in little-endian)
    0x00,        // bDeviceClass
    0x00,        // bDeviceSubClass
    0x00,        // bDeviceProtocol
    64,          // bMaxPacketSize0
    0x6D, 0x04,  // idVendor (0x046D in LE - Logitech)
    0x77, 0xC0,  // idProduct (0xC077 in LE - Optical USB Mouse)
    0x00, 0x01,  // bcdDevice (1.00)
    1,           // iManufacturer
    2,           // iProduct
    3,           // iSerialNumber
    1,           // bNumConfigurations
];

// Configuration Descriptor + Interface + HID + Endpoint (34 bytes total)
pub const FULL_CONFIG_DESC: [u8; 34] = [
    // Configuration Descriptor (9 bytes)
    9, 0x02, 34, 0x00, 1, 1, 0, 0xA0, 50,
    // Interface Descriptor (9 bytes): Class 3 (HID), SubClass 1 (Boot), Protocol 2 (Mouse)
    9, 0x04, 0, 0, 1, 0x03, 0x01, 0x02, 0,
    // HID Descriptor (9 bytes): Report Descriptor length = 52 (0x0034)
    9, 0x21, 0x11, 0x01, 0x00, 1, 0x22, 52, 0x00,
    // Endpoint Descriptor (7 bytes): EP 1 IN (0x81), Interrupt (0x03), max packet 64, interval 1ms
    7, 0x05, 0x81, 0x03, 64, 0x00, 1,
];

// USB String Descriptors
pub fn get_string_descriptor(index: u8) -> Vec<u8> {
    match index {
        0 => vec![4, 3, 0x09, 0x04], // English (US) Language ID
        1 => encode_utf16_desc("Logitech"),
        2 => encode_utf16_desc("Optical USB Mouse"),
        3 => encode_utf16_desc("00000001"),
        _ => Vec::new(),
    }
}

fn encode_utf16_desc(s: &str) -> Vec<u8> {
    let utf16: Vec<u16> = s.encode_utf16().collect();
    let length = (2 + utf16.len() * 2) as u8;
    let mut out = Vec::with_capacity(length as usize);
    out.push(length);
    out.push(0x03); // String descriptor type
    for code in utf16 {
        out.push((code & 0xFF) as u8);
        out.push((code >> 8) as u8);
    }
    out
}
