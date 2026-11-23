use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Duration;

use crate::descriptors::{get_string_descriptor, DEV_DESC, FULL_CONFIG_DESC, HID_REPORT_DESC};
use crate::logger::Logger;

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
    pub is_captured: Arc<AtomicBool>,
    pub is_counting_down: Arc<AtomicBool>,
    pub packets_sent: Arc<AtomicU64>,
    pub logger: Arc<Logger>,
}

impl UsbIpServer {
    pub fn new(
        host: &str,
        port: u16,
        is_captured: Arc<AtomicBool>,
        is_counting_down: Arc<AtomicBool>,
        logger: Arc<Logger>,
    ) -> Self {
        Self {
            host: host.to_string(),
            port,
            is_running: Arc::new(AtomicBool::new(true)),
            is_connected: Arc::new(AtomicBool::new(false)),
            is_captured,
            is_counting_down,
            packets_sent: Arc::new(AtomicU64::new(0)),
            logger,
        }
    }

    pub fn start(&self, mouse_rx: Receiver<MouseReport>) {
        // Start UDP auto-discovery responder on the same port
        let is_running_udp = self.is_running.clone();
        let udp_port = self.port;
        std::thread::spawn(move || {
            if let Ok(socket) = std::net::UdpSocket::bind(format!("0.0.0.0:{}", udp_port)) {
                let _ = socket.set_read_timeout(Some(Duration::from_millis(500)));
                let mut buf = [0u8; 64];
                while is_running_udp.load(Ordering::Relaxed) {
                    if let Ok((len, src)) = socket.recv_from(&mut buf) {
                        let msg = String::from_utf8_lossy(&buf[..len]);
                        if msg.starts_with("VM_DISCOVER") {
                            let _ = socket.send_to(b"VM_OFFER", src);
                        }
                    }
                }
            }
        });

        let addr = format!("{}:{}", self.host, self.port);
        let listener = match TcpListener::bind(&addr) {
            Ok(l) => {
                self.logger.log("USBIP", &format!("Listening on {}...", addr));
                l
            }
            Err(e) => {
                self.logger.log("USBIP_ERR", &format!("Failed to bind TCP listener on {}: {}", addr, e));
                return;
            }
        };

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
                        self.logger.log("USBIP_ERR", &format!("Unsupported protocol version: 0x{:04X} from {}", version, peer_addr));
                        continue;
                    }

                    match opcode {
                        OP_REQ_DEVLIST => {
                            self.logger.log_file_only("USBIP", &format!("Received OP_REQ_DEVLIST from {}", peer_addr));
                            self.handle_devlist(&mut stream);
                        }
                        OP_REQ_IMPORT => {
                            let mut busid_buf = [0u8; 32];
                            if stream.read_exact(&mut busid_buf).is_ok() {
                                let busid = String::from_utf8_lossy(&busid_buf)
                                    .trim_matches(char::from(0))
                                    .to_string();

                                self.logger.log("USBIP", &format!("Host connected from {}! Importing device busid '{}'...", peer_addr, busid));
                                self.is_connected.store(true, Ordering::SeqCst);

                                let is_connected = self.is_connected.clone();
                                let is_captured = self.is_captured.clone();
                                let is_counting_down = self.is_counting_down.clone();
                                let packets_sent = self.packets_sent.clone();
                                let is_running = self.is_running.clone();
                                let rx_clone = mouse_rx.clone();
                                let log = self.logger.clone();

                                std::thread::spawn(move || {
                                    Self::handle_import(
                                        stream,
                                        rx_clone,
                                        is_connected,
                                        is_captured,
                                        is_counting_down,
                                        packets_sent,
                                        is_running,
                                        log,
                                    );
                                });
                            }
                        }
                        _ => {
                            self.logger.log("USBIP_ERR", &format!("Unknown opcode: 0x{:04X}", opcode));
                        }
                    }
                }
                Err(e) => {
                    if !self.is_running.load(Ordering::SeqCst) {
                        break;
                    }
                    self.logger.log("USBIP_ERR", &format!("Accept error: {}", e));
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
        is_captured: Arc<AtomicBool>,
        is_counting_down: Arc<AtomicBool>,
        packets_sent: Arc<AtomicU64>,
        is_running: Arc<AtomicBool>,
        logger: Arc<Logger>,
    ) {
        // Send OP_REP_IMPORT
        let mut resp = Vec::with_capacity(8 + 312);
        resp.extend_from_slice(&USBIP_VERSION.to_be_bytes());
        resp.extend_from_slice(&OP_REP_IMPORT.to_be_bytes());
        resp.extend_from_slice(&0u32.to_be_bytes()); // status 0
        resp.extend_from_slice(&Self::build_device_info());

        if let Err(e) = stream.write_all(&resp) {
            logger.log("USBIP_ERR", &format!("Failed to send OP_REP_IMPORT: {}", e));
            is_connected.store(false, Ordering::SeqCst);
            return;
        }

        logger.log("USBIP", "Device imported! Host attached successfully.");

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
                    Self::handle_control_urb(&mut stream, seqnum, length, setup, &logger);
                } else if ep == 1 && direction == 1 {
                    // Mouse Interrupt IN URB
                    Self::handle_mouse_urb(&mut stream, seqnum, length, &mouse_rx, &mut last_buttons, &packets_sent, &logger);
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
                logger.log("USBIP_ERR", &format!("Unknown URB command: 0x{:08X}", cmd));
                break;
            }
        }

        logger.log("USBIP", "Host disconnected.");
        is_connected.store(false, Ordering::SeqCst);

        // Cancel countdown if it was in progress
        if is_counting_down.swap(false, Ordering::SeqCst) {
            logger.log("CAPTURE", "Host disconnected! Countdown cancelled.");
        }

        // Auto-release mouse capture back to local Server PC
        if is_captured.swap(false, Ordering::SeqCst) {
            logger.log("CAPTURE", "Host disconnected! Automatically returned mouse control to local Server PC.");
            crate::capture::sound_beep(600, 100);
            crate::capture::sound_beep(400, 150);
        }
    }

    fn handle_control_urb(stream: &mut TcpStream, seqnum: u32, length: i32, setup: &[u8], logger: &Arc<Logger>) {
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
                        1 => {
                            logger.log_file_only("EP0", "GET_DESCRIPTOR (Device) -> 18 bytes");
                            payload.extend_from_slice(&DEV_DESC);
                        }
                        2 => {
                            logger.log_file_only("EP0", "GET_DESCRIPTOR (Configuration) -> 34 bytes");
                            payload.extend_from_slice(&FULL_CONFIG_DESC);
                        }
                        3 => {
                            logger.log_file_only("EP0", &format!("GET_DESCRIPTOR (String index {})", desc_index));
                            payload.extend_from_slice(&get_string_descriptor(desc_index));
                        }
                        0x22 => {
                            logger.log_file_only("EP0", "GET_DESCRIPTOR (HID Report) -> 62 bytes");
                            payload.extend_from_slice(HID_REPORT_DESC);
                        }
                        _ => {}
                    }
                }
                0x09 => {
                    logger.log_file_only("EP0", "SET_CONFIGURATION -> Success");
                }
                0x00 => {
                    payload.extend_from_slice(&[0x00, 0x00]);
                }
                _ => {}
            }
        } else if req_type == 1 {
            // HID Class Request
            match b_request {
                0x0A => {
                    logger.log_file_only("EP0", "SET_IDLE -> Success");
                }
                0x01 => {
                    payload.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
                }
                0x09 => {}
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
        _packets_sent: &Arc<AtomicU64>,
        _logger: &Arc<Logger>,
    ) {
        let mut report_bytes = [*last_buttons, 0, 0, 0];

        // Wait up to 8ms for incoming mouse event (125Hz polling rate)
        if let Ok(rx) = mouse_rx.lock() {
            if let Ok(mut report) = rx.recv_timeout(Duration::from_millis(8)) {
                // Drain and coalesce any queued reports so movement is zero-latency & smooth
                while let Ok(next) = rx.try_recv() {
                    report.dx = ((report.dx as i16) + (next.dx as i16)).clamp(-127, 127) as i8;
                    report.dy = ((report.dy as i16) + (next.dy as i16)).clamp(-127, 127) as i8;
                    report.wheel = ((report.wheel as i16) + (next.wheel as i16)).clamp(-127, 127) as i8;
                    report.buttons = next.buttons;
                }
                *last_buttons = report.buttons;
                report_bytes = [
                    report.buttons,
                    report.dx as u8,
                    report.dy as u8,
                    report.wheel as u8,
                ];
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
