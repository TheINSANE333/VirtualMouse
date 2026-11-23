use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command};
use std::thread::sleep;
use std::time::Duration;

struct TestServer {
    child: Child,
    #[allow(dead_code)]
    port: u16,
}

impl TestServer {
    fn start(port: u16) -> Self {
        // Find executable (debug or release)
        let exe = if std::path::Path::new("target/release/virtual_mouse_server.exe").exists() {
            "target/release/virtual_mouse_server.exe"
        } else if std::path::Path::new("target/debug/virtual_mouse_server.exe").exists() {
            "target/debug/virtual_mouse_server.exe"
        } else if std::path::Path::new("VirtualMouseServer.exe").exists() {
            "VirtualMouseServer.exe"
        } else {
            panic!("Server executable not found. Run cargo build first.");
        };

        let child = Command::new(exe)
            .args(["--port", &port.to_string(), "--delay", "5"])
            .spawn()
            .expect("Failed to start server process");

        sleep(Duration::from_millis(600));

        Self { child, port }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

#[test]
fn test_usbip_devlist_and_import() {
    let port = 13248;
    let _server = TestServer::start(port);

    // 1. Test OP_REQ_DEVLIST
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .expect("Failed to connect to USB/IP server");

    let mut req = Vec::new();
    req.extend_from_slice(&0x0111u16.to_be_bytes()); // version
    req.extend_from_slice(&0x8005u16.to_be_bytes()); // OP_REQ_DEVLIST
    req.extend_from_slice(&0u32.to_be_bytes());     // status 0
    stream.write_all(&req).unwrap();

    let mut rep_header = [0u8; 12];
    stream.read_exact(&mut rep_header).unwrap();
    let version = u16::from_be_bytes([rep_header[0], rep_header[1]]);
    let opcode = u16::from_be_bytes([rep_header[2], rep_header[3]]);
    let status = u32::from_be_bytes([rep_header[4], rep_header[5], rep_header[6], rep_header[7]]);
    let ndev = u32::from_be_bytes([rep_header[8], rep_header[9], rep_header[10], rep_header[11]]);

    assert_eq!(version, 0x0111);
    assert_eq!(opcode, 0x0005);
    assert_eq!(status, 0);
    assert_eq!(ndev, 1);

    let mut dev_bytes = [0u8; 312];
    stream.read_exact(&mut dev_bytes).unwrap();
    let busid = &dev_bytes[256..259];
    assert_eq!(busid, b"1-1");

    let mut if_bytes = [0u8; 4];
    stream.read_exact(&mut if_bytes).unwrap();
    assert_eq!(if_bytes[0], 0x03); // HID
    assert_eq!(if_bytes[2], 0x02); // Mouse

    drop(stream);

    // 2. Test OP_REQ_IMPORT
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .expect("Failed to connect to USB/IP server");

    let mut req = Vec::new();
    req.extend_from_slice(&0x0111u16.to_be_bytes()); // version
    req.extend_from_slice(&0x8003u16.to_be_bytes()); // OP_REQ_IMPORT
    req.extend_from_slice(&0u32.to_be_bytes());     // status 0
    let mut busid_pad = [0u8; 32];
    busid_pad[..3].copy_from_slice(b"1-1");
    req.extend_from_slice(&busid_pad);
    stream.write_all(&req).unwrap();

    let mut rep_header = [0u8; 8];
    stream.read_exact(&mut rep_header).unwrap();
    let version = u16::from_be_bytes([rep_header[0], rep_header[1]]);
    let opcode = u16::from_be_bytes([rep_header[2], rep_header[3]]);
    let status = u32::from_be_bytes([rep_header[4], rep_header[5], rep_header[6], rep_header[7]]);

    assert_eq!(version, 0x0111);
    assert_eq!(opcode, 0x0003);
    assert_eq!(status, 0);

    let mut dev_bytes = [0u8; 312];
    stream.read_exact(&mut dev_bytes).unwrap();
    assert_eq!(&dev_bytes[256..259], b"1-1");

    // 3. Test Control URB (GET_DESCRIPTOR Device)
    let mut setup = [0u8; 8];
    setup[0] = 0x80; // bmRequestType
    setup[1] = 0x06; // bRequest = GET_DESCRIPTOR
    setup[2..4].copy_from_slice(&0x0100u16.to_le_bytes()); // wValue = Device
    setup[4..6].copy_from_slice(&0x0000u16.to_le_bytes()); // wIndex = 0
    setup[6..8].copy_from_slice(&18u16.to_le_bytes());     // wLength = 18

    let mut cmd_submit = Vec::new();
    cmd_submit.extend_from_slice(&0x00000001u32.to_be_bytes()); // USBIP_CMD_SUBMIT
    cmd_submit.extend_from_slice(&1u32.to_be_bytes());        // seqnum
    cmd_submit.extend_from_slice(&0u32.to_be_bytes());        // devid
    cmd_submit.extend_from_slice(&1u32.to_be_bytes());        // direction (IN)
    cmd_submit.extend_from_slice(&0u32.to_be_bytes());        // ep 0
    cmd_submit.extend_from_slice(&0u32.to_be_bytes());        // flags
    cmd_submit.extend_from_slice(&18i32.to_be_bytes());       // length
    cmd_submit.extend_from_slice(&0i32.to_be_bytes());        // start_frame
    cmd_submit.extend_from_slice(&0i32.to_be_bytes());        // number_of_packets
    cmd_submit.extend_from_slice(&0i32.to_be_bytes());        // interval
    cmd_submit.extend_from_slice(&setup);

    stream.write_all(&cmd_submit).unwrap();

    let mut ret_submit = [0u8; 48];
    stream.read_exact(&mut ret_submit).unwrap();
    let ret_cmd = u32::from_be_bytes([ret_submit[0], ret_submit[1], ret_submit[2], ret_submit[3]]);
    let ret_seq = u32::from_be_bytes([ret_submit[4], ret_submit[5], ret_submit[6], ret_submit[7]]);
    let ret_len = i32::from_be_bytes([ret_submit[24], ret_submit[25], ret_submit[26], ret_submit[27]]);

    assert_eq!(ret_cmd, 0x00000003); // USBIP_RET_SUBMIT
    assert_eq!(ret_seq, 1);
    assert_eq!(ret_len, 18);

    let mut dev_desc = [0u8; 18];
    stream.read_exact(&mut dev_desc).unwrap();
    assert_eq!(&dev_desc[8..12], &[0x6D, 0x04, 0x77, 0xC0]); // 0x046D:0xC077
}
