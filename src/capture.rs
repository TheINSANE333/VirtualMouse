#![allow(non_snake_case)]
#![allow(dead_code)]

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use crate::logger::Logger;
use crate::usbip::MouseReport;

// Win32 Constants
const RID_INPUT: u32 = 0x10000003;
const WM_INPUT: u32 = 0x00FF;
const RIDEV_INPUTSINK: u32 = 0x00000100;
const WH_MOUSE_LL: i32 = 14;

// Raw Input Button Flags
const RI_MOUSE_LEFT_BUTTON_DOWN: u16 = 0x0001;
const RI_MOUSE_LEFT_BUTTON_UP: u16 = 0x0002;
const RI_MOUSE_RIGHT_BUTTON_DOWN: u16 = 0x0004;
const RI_MOUSE_RIGHT_BUTTON_UP: u16 = 0x0008;
const RI_MOUSE_MIDDLE_BUTTON_DOWN: u16 = 0x0010;
const RI_MOUSE_MIDDLE_BUTTON_UP: u16 = 0x0020;
const RI_MOUSE_BUTTON_4_DOWN: u16 = 0x0040;
const RI_MOUSE_BUTTON_4_UP: u16 = 0x0080;
const RI_MOUSE_BUTTON_5_DOWN: u16 = 0x0100;
const RI_MOUSE_BUTTON_5_UP: u16 = 0x0200;
const RI_MOUSE_WHEEL: u16 = 0x0400;

#[repr(C)]
pub struct WNDCLASSEXW {
    pub cbSize: u32,
    pub style: u32,
    pub lpfnWndProc: Option<unsafe extern "system" fn(isize, u32, usize, isize) -> isize>,
    pub cbClsExtra: i32,
    pub cbWndExtra: i32,
    pub hInstance: isize,
    pub hIcon: isize,
    pub hCursor: isize,
    pub hbrBackground: isize,
    pub lpszMenuName: *const u16,
    pub lpszClassName: *const u16,
    pub hIconSm: isize,
}

#[repr(C)]
pub struct RAWINPUTDEVICE {
    pub usUsagePage: u16,
    pub usUsage: u16,
    pub dwFlags: u32,
    pub hwndTarget: isize,
}

#[repr(C)]
pub struct RAWINPUTHEADER {
    pub dwType: u32,
    pub dwSize: u32,
    pub hDevice: isize,
    pub wParam: usize,
}

#[repr(C)]
pub struct RAWMOUSE {
    pub usFlags: u16,
    pub pad: u16,
    pub usButtonFlags: u16,
    pub usButtonData: u16,
    pub ulRawButtons: u32,
    pub lLastX: i32,
    pub lLastY: i32,
    pub ulExtraInformation: u32,
}

#[repr(C)]
pub struct RAWINPUT {
    pub header: RAWINPUTHEADER,
    pub mouse: RAWMOUSE,
}

#[repr(C)]
pub struct MSG {
    pub hwnd: isize,
    pub message: u32,
    pub wParam: usize,
    pub lParam: isize,
    pub time: u32,
    pub pt_x: i32,
    pub pt_y: i32,
}

#[link(name = "user32")]
unsafe extern "system" {
    fn RegisterClassExW(lpWndClass: *const WNDCLASSEXW) -> u16;
    fn CreateWindowExW(
        dwExStyle: u32,
        lpClassName: *const u16,
        lpWindowName: *const u16,
        dwStyle: u32,
        X: i32,
        Y: i32,
        nWidth: i32,
        nHeight: i32,
        hWndParent: isize,
        hMenu: isize,
        hInstance: isize,
        lpParam: *mut c_void,
    ) -> isize;
    fn DefWindowProcW(hWnd: isize, Msg: u32, wParam: usize, lParam: isize) -> isize;
    fn RegisterRawInputDevices(
        pRawInputDevices: *const RAWINPUTDEVICE,
        uiNumDevices: u32,
        cbSize: u32,
    ) -> i32;
    fn GetRawInputData(
        hRawInput: isize,
        uiCommand: u32,
        pData: *mut c_void,
        pcbSize: *mut u32,
        cbSizeHeader: u32,
    ) -> u32;
    fn SetWindowsHookExW(
        idHook: i32,
        lpfn: Option<unsafe extern "system" fn(i32, usize, isize) -> isize>,
        hmod: isize,
        dwThreadId: u32,
    ) -> isize;
    fn UnhookWindowsHookEx(hhk: isize) -> i32;
    fn CallNextHookEx(hhk: isize, nCode: i32, wParam: usize, lParam: isize) -> isize;
    fn GetMessageW(lpMsg: *mut MSG, hWnd: isize, wMsgFilterMin: u32, wMsgFilterMax: u32) -> i32;
    fn TranslateMessage(lpMsg: *const MSG) -> i32;
    fn DispatchMessageW(lpMsg: *const MSG) -> isize;
    fn GetAsyncKeyState(vKey: i32) -> i16;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(lpModuleName: *const u16) -> isize;
    pub fn Beep(dwFreq: u32, dwDuration: u32) -> i32;
    fn GetLastError() -> u32;
}

struct CaptureState {
    mouse_tx: SyncSender<MouseReport>,
    is_captured: Arc<AtomicBool>,
    is_counting_down: Arc<AtomicBool>,
    current_buttons: AtomicU8,
    logger: Arc<Logger>,
}

static STATE: OnceLock<CaptureState> = OnceLock::new();

pub fn sound_beep(freq: u32, dur: u32) {
    unsafe {
        Beep(freq, dur);
    }
}

pub fn trigger_toggle(
    is_captured: &Arc<AtomicBool>,
    is_counting_down: &Arc<AtomicBool>,
    delay_secs: u64,
    logger: &Arc<Logger>,
) {
    if is_captured.load(Ordering::SeqCst) {
        // Release capture
        is_captured.store(false, Ordering::SeqCst);
        logger.log("CAPTURE", "RELEASED! Mouse control returned to local Server PC.");
        sound_beep(600, 100);
        sound_beep(400, 150);
    } else if is_counting_down.load(Ordering::SeqCst) {
        // Cancel countdown
        is_counting_down.store(false, Ordering::SeqCst);
        logger.log("CAPTURE", "Countdown cancelled.");
        sound_beep(400, 200);
    } else {
        // Start countdown
        is_counting_down.store(true, Ordering::SeqCst);
        let captured = is_captured.clone();
        let counting = is_counting_down.clone();
        let log = logger.clone();

        log.log("CAPTURE", &format!("Hotkey triggered! Starting {}s countdown...", delay_secs));

        std::thread::spawn(move || {
            for sec in (1..=delay_secs).rev() {
                if !counting.load(Ordering::SeqCst) {
                    return;
                }
                log.log("COUNTDOWN", &format!("Starting in {}s... Focus your Remote Desktop now!", sec));
                sound_beep(700 + ((delay_secs - sec) as u32) * 90, 80);
                std::thread::sleep(Duration::from_millis(920));
            }

            if !counting.load(Ordering::SeqCst) {
                return;
            }
            counting.store(false, Ordering::SeqCst);
            captured.store(true, Ordering::SeqCst);

            log.log("CAPTURE", "CAPTURED! Pure raw hardware mouse input streaming directly to Host PC! (Press F12 to release)");
            sound_beep(1500, 250);
        });
    }
}

// Low-level mouse hook: only swallows mouse messages locally on Server PC so the local cursor stays frozen
unsafe extern "system" fn mouse_hook_proc(n_code: i32, w_param: usize, l_param: isize) -> isize {
    if n_code >= 0 {
        if let Some(state) = STATE.get() {
            if state.is_captured.load(Ordering::Relaxed) {
                // Return 1: drop mouse event on Server PC desktop
                return 1;
            }
        }
    }
    unsafe { CallNextHookEx(0, n_code, w_param, l_param) }
}

// Window procedure for hidden Raw Input receiver window
unsafe extern "system" fn wnd_proc(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize {
    if msg == WM_INPUT {
        if let Some(state) = STATE.get() {
            if state.is_captured.load(Ordering::Relaxed) {
                let mut size: u32 = std::mem::size_of::<RAWINPUT>() as u32;
                let mut raw: RAWINPUT = unsafe { std::mem::zeroed() };

                let res = unsafe {
                    GetRawInputData(
                        lparam as isize,
                        RID_INPUT,
                        &mut raw as *mut _ as *mut c_void,
                        &mut size,
                        std::mem::size_of::<RAWINPUTHEADER>() as u32,
                    )
                };

                if res != u32::MAX && raw.header.dwType == 0 {
                    let mut dx: i8 = 0;
                    let mut dy: i8 = 0;
                    let mut send_report = false;

                    // MOUSE_MOVE_RELATIVE = 0x00. lLastX and lLastY are pure physical hardware sensor deltas!
                    if (raw.mouse.usFlags & 0x01) == 0 {
                        let rx = raw.mouse.lLastX;
                        let ry = raw.mouse.lLastY;
                        if rx != 0 || ry != 0 {
                            dx = rx.clamp(-127, 127) as i8;
                            dy = ry.clamp(-127, 127) as i8;
                            send_report = true;
                        }
                    }

                    let bflags = raw.mouse.usButtonFlags;
                    let mut btn = state.current_buttons.load(Ordering::Relaxed);
                    let mut wheel: i8 = 0;

                    if bflags & RI_MOUSE_LEFT_BUTTON_DOWN != 0 {
                        btn |= 1;
                        send_report = true;
                    }
                    if bflags & RI_MOUSE_LEFT_BUTTON_UP != 0 {
                        btn &= !1;
                        send_report = true;
                    }
                    if bflags & RI_MOUSE_RIGHT_BUTTON_DOWN != 0 {
                        btn |= 2;
                        send_report = true;
                    }
                    if bflags & RI_MOUSE_RIGHT_BUTTON_UP != 0 {
                        btn &= !2;
                        send_report = true;
                    }
                    if bflags & RI_MOUSE_MIDDLE_BUTTON_DOWN != 0 {
                        btn |= 4;
                        send_report = true;
                    }
                    if bflags & RI_MOUSE_MIDDLE_BUTTON_UP != 0 {
                        btn &= !4;
                        send_report = true;
                    }
                    if bflags & RI_MOUSE_BUTTON_4_DOWN != 0 {
                        btn |= 8;
                        send_report = true;
                    }
                    if bflags & RI_MOUSE_BUTTON_4_UP != 0 {
                        btn &= !8;
                        send_report = true;
                    }
                    if bflags & RI_MOUSE_BUTTON_5_DOWN != 0 {
                        btn |= 16;
                        send_report = true;
                    }
                    if bflags & RI_MOUSE_BUTTON_5_UP != 0 {
                        btn &= !16;
                        send_report = true;
                    }
                    if bflags & RI_MOUSE_WHEEL != 0 {
                        let w_raw = (raw.mouse.usButtonData as i16) / 120;
                        wheel = w_raw.clamp(-127, 127) as i8;
                        send_report = true;
                    }

                    if send_report {
                        state.current_buttons.store(btn, Ordering::Relaxed);
                        let report = MouseReport {
                            buttons: btn,
                            dx,
                            dy,
                            wheel,
                        };
                        let _ = state.mouse_tx.try_send(report);
                    }
                }
            }
        }
    }

    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn start_capture_engine(
    mouse_tx: SyncSender<MouseReport>,
    is_captured: Arc<AtomicBool>,
    is_counting_down: Arc<AtomicBool>,
    is_running: Arc<AtomicBool>,
    delay_secs: u64,
    logger: Arc<Logger>,
) {
    let _ = STATE.set(CaptureState {
        mouse_tx,
        is_captured: is_captured.clone(),
        is_counting_down: is_counting_down.clone(),
        current_buttons: AtomicU8::new(0),
        logger: logger.clone(),
    });

    // 1. Start Global Hotkey Poller Thread (F12, F11, ScrollLock)
    let hotkey_captured = is_captured.clone();
    let hotkey_counting = is_counting_down.clone();
    let hotkey_running = is_running.clone();
    let hotkey_log = logger.clone();

    std::thread::spawn(move || {
        let mut f12_was_down = false;
        let mut f11_was_down = false;
        let mut scroll_was_down = false;

        hotkey_log.log("HOTKEY", "Monitoring global hotkeys: [ F12 ] / [ F11 ] / [ Scroll Lock ]");

        while hotkey_running.load(Ordering::Relaxed) {
            unsafe {
                // VK_F12 = 0x7B
                let f12_down = (GetAsyncKeyState(0x7B) as u16 & 0x8000) != 0;
                if f12_down && !f12_was_down {
                    hotkey_log.log_file_only("HOTKEY", "Key [ F12 ] pressed!");
                    trigger_toggle(&hotkey_captured, &hotkey_counting, delay_secs, &hotkey_log);
                }
                f12_was_down = f12_down;

                // VK_F11 = 0x7A (backup hotkey)
                let f11_down = (GetAsyncKeyState(0x7A) as u16 & 0x8000) != 0;
                if f11_down && !f11_was_down {
                    hotkey_log.log_file_only("HOTKEY", "Key [ F11 ] pressed!");
                    trigger_toggle(&hotkey_captured, &hotkey_counting, delay_secs, &hotkey_log);
                }
                f11_was_down = f11_down;

                // VK_SCROLL = 0x91 (Scroll Lock)
                let scroll_down = (GetAsyncKeyState(0x91) as u16 & 0x8000) != 0;
                if scroll_down && !scroll_was_down {
                    hotkey_log.log_file_only("HOTKEY", "Key [ Scroll Lock ] pressed!");
                    trigger_toggle(&hotkey_captured, &hotkey_counting, delay_secs, &hotkey_log);
                }
                scroll_was_down = scroll_down;
            }

            std::thread::sleep(Duration::from_millis(15));
        }
    });

    // 2. Start Console Key Listener Thread (Pressing Enter in console also toggles)
    let console_captured = is_captured.clone();
    let console_counting = is_counting_down.clone();
    let console_running = is_running.clone();
    let console_log = logger.clone();

    std::thread::spawn(move || {
        use std::io::BufRead;
        let stdin = std::io::stdin();
        let mut lines = stdin.lock().lines();

        while console_running.load(Ordering::Relaxed) {
            if let Some(Ok(_)) = lines.next() {
                console_log.log_file_only("CONSOLE", "Console [ Enter ] pressed!");
                trigger_toggle(&console_captured, &console_counting, delay_secs, &console_log);
            }
        }
    });

    // 3. Register Raw Input on a hidden Top-Level Window + Install WH_MOUSE_LL swallow hook
    unsafe {
        let class_name = to_wide_null("VirtualMouseServerRawReceiver");
        let h_inst = GetModuleHandleW(std::ptr::null());

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: h_inst,
            hIcon: 0,
            hCursor: 0,
            hbrBackground: 0,
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
            hIconSm: 0,
        };

        RegisterClassExW(&wc);

        // Hidden top-level window (NOT WS_VISIBLE, 0x0 size) -> Receives RIDEV_INPUTSINK globally!
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            class_name.as_ptr(),
            0, // NOT WS_VISIBLE
            0, 0, 0, 0,
            0, // Top-level
            0,
            h_inst,
            std::ptr::null_mut(),
        );

        if hwnd == 0 {
            let err = GetLastError();
            logger.log("RAW_ERR", &format!("CRITICAL: Failed to create raw input receiver window: {}", err));
            return;
        }

        // Register Mouse with RIDEV_INPUTSINK (0x00000100)
        let rid = RAWINPUTDEVICE {
            usUsagePage: 1,      // Generic Desktop
            usUsage: 2,          // Mouse
            dwFlags: RIDEV_INPUTSINK,
            hwndTarget: hwnd,
        };

        if RegisterRawInputDevices(&rid, 1, std::mem::size_of::<RAWINPUTDEVICE>() as u32) == 0 {
            let err = GetLastError();
            logger.log("RAW_ERR", &format!("CRITICAL: RegisterRawInputDevices failed: {}", err));
            return;
        }

        logger.log("RAW", "Hardware raw mouse capture registered successfully (RIDEV_INPUTSINK).");

        // Install WH_MOUSE_LL to swallow inputs locally on Server PC while captured
        let hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), h_inst, 0);
        if hook == 0 {
            let err = GetLastError();
            logger.log("HOOK_ERR", &format!("CRITICAL: SetWindowsHookExW failed with error code: {}", err));
            return;
        }
        logger.log("HOOK", &format!("Low-level mouse hook installed (handle: {})", hook));

        let mut msg: MSG = std::mem::zeroed();
        while is_running.load(Ordering::Relaxed) {
            let res = GetMessageW(&mut msg, 0, 0, 0);
            if res <= 0 {
                break;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        UnhookWindowsHookEx(hook);
        logger.log("HOOK", "Low-level mouse hook removed.");
    }
}
