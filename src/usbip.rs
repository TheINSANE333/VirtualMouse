use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Duration;

use crate::descriptors::{get_string_descriptor, DEV_DESC, FULL_CONFIG_DESC, HID_REPORT_DESC};

pub const USBIP_PORT: u16 = 3240;
pub const USBIP_VERSION: u16 = 0x0111;

pub const OP_REQ_DEVLIST: u16 = 0x8005;
pub const OP_REP_DEVLIST: u16 = 0x0005;
pub const OP_REQ_IMPORT: u16 = 0x8003;
pub const OP_REP_IMPORT: u16 = 0x0003;

pub const USBIP_CMD_SUBMIT: u32 = 0x00000001;
pub const USBIP_RET_SUBMIT: u32 = 0x00000003;
pub const USBIP_CMD_UNLINK: u32 = 0x00000002;
pub const USBIP_RET_UNLINK: u32 = 0x00000004;

#[derive(Debug, Clone, Copy, Default)]
pub struct MouseReport {
    pub buttons: u8,
    pub dx: i8,
    pub dy: i8,
    pub wheel: i8,
}

pub struct UsbIpServer {
    pub host: String,
    pub port: u16,
    pub is_running: Arc<AtomicBool>,
    pub is_connected: Arc<AtomicBool>,
    pub packets_sent: Arc<AtomicU64>,
}

impl UsbIpServer {
    pub fn new(host: &str, port: u16) -> Self {
        Self {
            host: host.to_string(),
            port,
            is_running: Arc::new(AtomicBool::new(true)),
            is_connected: Arc::new(AtomicBool::new(false)),
            packets_sent: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn start(&self, mouse_rx: Receiver<MouseReport>) {
        let addr = format!("{}:{}", self.host, self.port);
        let listener = match TcpListener::bind(&addr) {
            Ok(l) => {
                println!("[USB/IP Server] Listening on {}...", addr);
                l
            }
            Err(e) => {
                eprintln!("[!] Failed to bind TCP listener on {}: {}", addr, e);
                return;
            }
        };

        // Channel receiver needs to be shared or handled when client connects
        let mouse_rx = Arc::new(std::sync::Mutex::new(mouse_rx));

        while self.is_running.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut stream, peer_addr)) => {
                    let mut header = [0u8; 8];
                    if stream.read_exact(&mut header).is_err() {
                        continue;
                    }

                    let version = u16::from_be_bytes([header[0], header[1]]);
                    let opcode = u16::from_be_bytes([header[2], header[3]]);
                    let _status = u32::from_be_bytes([header[4], header[5], header[6], header[7]]);

                    if version != USBIP_VERSION && version != 0x0106 {
                        eprintln!("[USB/IP] Unsupported protocol version: 0x{:04X}", version);
                        continue;
                    }

                    match opcode {
                        OP_REQ_DEVLIST => {
                            self.handle_devlist(&mut stream);
                        }
                        OP_REQ_IMPORT => {
                            let mut busid_buf = [0u8; 32];
                            if stream.read_exact(&mut busid_buf).is_ok() {
                                println!("\n[USB/IP] Host connected from {}! Importing mouse...", peer_addr);
                                self.is_connected.store(true, Ordering::SeqCst);

                                let is_connected = self.is_connected.clone();
                                let packets_sent = self.packets_sent.clone();
                                let is_running = self.is_running.clone();
                                let rx_clone = mouse_rx.clone();

                                std::thread::spawn(move || {
                                    Self::handle_import(stream, rx_clone, is_connected, packets_sent, is_running);
                                });
                            }
                        }
                        _ => {
                            eprintln!("[USB/IP] Unknown opcode: 0x{:04X}", opcode);
                        }
                    }
                }
                Err(e) => {
                    if !self.is_running.load(Ordering::SeqCst) {
                        break;
                    }
                    eprintln!("[USB/IP] Accept error: {}", e);
                }
            }
        }
    }

    fn build_device_info() -> [u8; 312] {
        let mut buf = [0u8; 312];
        let path = b"/sys/devices/pci0000:00/0000:00:01.2/usb1/1-1";
        buf[..path.len()].copy_from_slice(path);

        let busid = b"1-1";
        buf[256..256 + busid.len()].copy_from_slice(busid);

        buf[288..292].copy_from_slice(&1u32.to_be_bytes()); // busnum
        buf[292..296].copy_from_slice(&2u32.to_be_bytes()); // devnum
        buf[296..300].copy_from_slice(&2u32.to_be_bytes()); // speed (high)

        buf[300..302].copy_from_slice(&0x046Du16.to_be_bytes()); // idVendor
        buf[302..304].copy_from_slice(&0xC077u16.to_be_bytes()); // idProduct
        buf[304..306].copy_from_slice(&0x0100u16.to_be_bytes()); // bcdDevice

        buf[306] = 0; // bDeviceClass
        buf[307] = 0; // bDeviceSubClass
        buf[308] = 0; // bDeviceProtocol
        buf[309] = 1; // bConfigurationValue
        buf[310] = 1; // bNumConfigurations
        buf[311] = 1; // bNumInterfaces

        buf
    }

    fn handle_devlist(&self, stream: &mut TcpStream) {
        let mut resp = Vec::with_capacity(8 + 4 + 312 + 4);
        resp.extend_from_slice(&USBIP_VERSION.to_be_bytes());
        resp.extend_from_slice(&OP_REP_DEVLIST.to_be_bytes());
        resp.extend_from_slice(&0u32.to_be_bytes()); // status 0

        resp.extend_from_slice(&1u32.to_be_bytes()); // ndev = 1
        resp.extend_from_slice(&Self::build_device_info());

        // Interface 0 (4 bytes): HID Class 3, Boot Subclass 1, Mouse Protocol 2, padding 0
        resp.extend_from_slice(&[0x03, 0x01, 0x02, 0x00]);

        let _ = stream.write_all(&resp);
    }

    fn handle_import(
        mut stream: TcpStream,
        mouse_rx: Arc<std::sync::Mutex<Receiver<MouseReport>>>,
        is_connected: Arc<AtomicBool>,
        packets_sent: Arc<AtomicU64>,
        is_running: Arc<AtomicBool>,
    ) {
        // Send OP_REP_IMPORT
        let mut resp = Vec::with_capacity(8 + 312);
        resp.extend_from_slice(&USBIP_VERSION.to_be_bytes());
        resp.extend_from_slice(&OP_REP_IMPORT.to_be_bytes());
        resp.extend_from_slice(&0u32.to_be_bytes()); // status 0
        resp.extend_from_slice(&Self::build_device_info());

        if let Err(e) = stream.write_all(&resp) {
            eprintln!("[USB/IP] Failed to send OP_REP_IMPORT: {}", e);
            is_connected.store(false, Ordering::SeqCst);
            return;
        }

        println!("[USB/IP] Device imported! Host attached successfully.");

        let mut last_buttons = 0u8;

        // URB packet loop
        while is_running.load(Ordering::SeqCst) {
            let mut header = [0u8; 48];
            if stream.read_exact(&mut header).is_err() {
                break;
            }

            let cmd = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
            let seqnum = u32::from_be_bytes([header[4], header[5], header[6], header[7]]);
            let _devid = u32::from_be_bytes([header[8], header[9], header[10], header[11]]);
            let direction = u32::from_be_bytes([header[12], header[13], header[14], header[15]]);
            let ep = u32::from_be_bytes([header[16], header[17], header[18], header[19]]);
            let length = i32::from_be_bytes([header[24], header[25], header[26], header[27]]);
            let setup = &header[40..48];

            if cmd == USBIP_CMD_SUBMIT {
                let mut out_payload = Vec::new();
                if direction == 0 && length > 0 {
                    out_payload.resize(length as usize, 0);
                    if stream.read_exact(&mut out_payload).is_err() {
                        break;
                    }
                }

                if ep == 0 {
                    // Control URB
                    Self::handle_control_urb(&mut stream, seqnum, length, setup);
                } else if ep == 1 && direction == 1 {
                    // Mouse Interrupt IN URB
                    Self::handle_mouse_urb(&mut stream, seqnum, length, &mouse_rx, &mut last_buttons, &packets_sent);
                } else {
                    Self::send_ret_submit(&mut stream, seqnum, 0, &[]);
                }
            } else if cmd == USBIP_CMD_UNLINK {
                let mut ret_unlink = [0u8; 48];
                ret_unlink[0..4].copy_from_slice(&USBIP_RET_UNLINK.to_be_bytes());
                ret_unlink[4..8].copy_from_slice(&seqnum.to_be_bytes());
                if stream.write_all(&ret_unlink).is_err() {
                    break;
                }
            } else {
                eprintln!("[USB/IP] Unknown URB command: 0x{:08X}", cmd);
                break;
            }
        }

        println!("[USB/IP] Host disconnected.");
        is_connected.store(false, Ordering::SeqCst);
    }

    fn handle_control_urb(stream: &mut TcpStream, seqnum: u32, length: i32, setup: &[u8]) {
        let bm_request_type = setup[0];
        let b_request = setup[1];
        let w_value = u16::from_le_bytes([setup[2], setup[3]]);
        let _w_index = u16::from_le_bytes([setup[4], setup[5]]);
        let _w_length = u16::from_le_bytes([setup[6], setup[7]]);

        let req_type = (bm_request_type >> 5) & 0x03;
        let mut payload = Vec::new();

        if req_type == 0 {
            // Standard Request
            match b_request {
                0x06 => {
                    // GET_DESCRIPTOR
                    let desc_type = (w_value >> 8) as u8;
                    let desc_index = (w_value & 0xFF) as u8;
                    match desc_type {
                        1 => payload.extend_from_slice(&DEV_DESC),
                        2 => payload.extend_from_slice(&FULL_CONFIG_DESC),
                        3 => payload.extend_from_slice(&get_string_descriptor(desc_index)),
                        0x22 => payload.extend_from_slice(HID_REPORT_DESC),
                        _ => {}
                    }
                }
                0x09 => {
                    // SET_CONFIGURATION
                }
                0x00 => {
                    // GET_STATUS
                    payload.extend_from_slice(&[0x00, 0x00]);
                }
                _ => {}
            }
        } else if req_type == 1 {
            // HID Class Request
            match b_request {
                0x0A => {
                    // SET_IDLE
                }
                0x01 => {
                    // GET_REPORT
                    payload.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
                }
                0x09 => {
                    // SET_REPORT
                }
                _ => {}
            }
        }

        if length > 0 && payload.len() > length as usize {
            payload.truncate(length as usize);
        }

        Self::send_ret_submit(stream, seqnum, 0, &payload);
    }

    fn handle_mouse_urb(
        stream: &mut TcpStream,
        seqnum: u32,
        length: i32,
        mouse_rx: &Arc<std::sync::Mutex<Receiver<MouseReport>>>,
        last_buttons: &mut u8,
        packets_sent: &Arc<AtomicU64>,
    ) {
        let mut report_bytes = [*last_buttons, 0, 0, 0];

        // Wait up to 8ms for incoming mouse event (125Hz polling rate)
        if let Ok(rx) = mouse_rx.lock() {
            if let Ok(report) = rx.recv_timeout(Duration::from_millis(8)) {
                *last_buttons = report.buttons;
                report_bytes = [
                    report.buttons,
                    report.dx as u8,
                    report.dy as u8,
                    report.wheel as u8,
                ];
                packets_sent.fetch_add(1, Ordering::Relaxed);
            }
        }

        let mut payload = &report_bytes[..];
        if length > 0 && payload.len() > length as usize {
            payload = &payload[..length as usize];
        }

        Self::send_ret_submit(stream, seqnum, 0, payload);
    }

    fn send_ret_submit(stream: &mut TcpStream, seqnum: u32, status: i32, payload: &[u8]) {
        let mut resp = Vec::with_capacity(48 + payload.len());
        resp.extend_from_slice(&USBIP_RET_SUBMIT.to_be_bytes());
        resp.extend_from_slice(&seqnum.to_be_bytes());
        resp.extend_from_slice(&0u32.to_be_bytes()); // devid
        resp.extend_from_slice(&0u32.to_be_bytes()); // direction
        resp.extend_from_slice(&0u32.to_be_bytes()); // ep
        resp.extend_from_slice(&status.to_be_bytes());
        resp.extend_from_slice(&(payload.len() as i32).to_be_bytes());
        resp.extend_from_slice(&0i32.to_be_bytes()); // start_frame
        resp.extend_from_slice(&0i32.to_be_bytes()); // number_of_packets
        resp.extend_from_slice(&0i32.to_be_bytes()); // error_count
        resp.extend_from_slice(&[0u8; 8]); // setup padding
        resp.extend_from_slice(payload);

        let _ = stream.write_all(&resp);
    }
}
