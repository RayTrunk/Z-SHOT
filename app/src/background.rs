// Z-SHOT — Copyright (C) 2026 R. Trunk
// SPDX-License-Identifier: GPL-3.0-or-later

//! Background thread: system-tray icon, global hotkeys and hotkey recording.
//!
//! The thread runs a Win32 message loop on a hidden window. The egui app talks to it
//! through [`send_command`] (queued + a posted wake-up message) and receives
//! [`BackgroundEvent`]s over an mpsc channel.

use crate::settings::{Hotkey, HotkeyAction};
#[cfg(not(any(windows, target_os = "linux")))]
use std::sync::mpsc::Sender;

/// Events sent from the background hotkey / system-tray thread to the egui app.
/// (Hotkey registration and recording exist on Windows only.)
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub enum BackgroundEvent {
    StartRegionCapture,
    StartFullCapture,
    ShowGui,
    OpenSettings,
    Quit,
    /// Result of (re-)registering the global hotkeys: `true` = registered,
    /// `false` = Windows refused (usually taken by another program).
    HotkeysApplied(Vec<(HotkeyAction, bool)>),
    /// Result of hotkey recording; `None` when cancelled with Esc.
    HotkeyRecorded(Option<Hotkey>),
}

impl BackgroundEvent {
    /// Events the UI must handle even while its window is hidden; the background
    /// thread wakes the window for them (see `window::wake`).
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn wakes_window(&self) -> bool {
        matches!(
            self,
            Self::StartRegionCapture
                | Self::StartFullCapture
                | Self::ShowGui
                | Self::OpenSettings
                | Self::Quit
        )
    }
}

/// Commands sent from the egui app to the background thread.
#[derive(Debug, Clone)]
#[cfg_attr(not(windows), allow(dead_code))]
pub enum BgCommand {
    SetHotkeys(Vec<(HotkeyAction, Hotkey)>),
    /// Unregister hotkeys and let the Z-SHOT window capture the next key combination
    /// pressed while it has focus (see `window::set_key_capture`).
    StartRecording,
    CancelRecording,
    /// Sent by the Z-SHOT window when it captured a combination (`None` = Esc).
    FinishRecording(Option<Hotkey>),
    Notify { title: String, text: String },
}

#[cfg(windows)]
pub use win::{send_command, start_background_listener};

#[cfg(target_os = "linux")]
pub use linux_ui::{send_command, start_background_listener};

#[cfg(not(any(windows, target_os = "linux")))]
pub fn start_background_listener(_tx: Sender<BackgroundEvent>, _ctx: eframe::egui::Context) {}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn send_command(_cmd: BgCommand) {}

/// Linux: tray and shortcuts live in the daemon process (`linux::daemon`); the
/// window process only receives its commands over `ui.sock`.
#[cfg(target_os = "linux")]
mod linux_ui {
    use super::{BackgroundEvent, BgCommand};
    use crate::linux::ipc::{self, Command, ListenError, Socket};
    use std::sync::mpsc::Sender;

    pub fn start_background_listener(tx: Sender<BackgroundEvent>, ctx: eframe::egui::Context) {
        let result = ipc::listen(Socket::Ui, move |cmd| {
            let ev = match cmd {
                Command::Region => BackgroundEvent::StartRegionCapture,
                Command::Full => BackgroundEvent::StartFullCapture,
                Command::Show => BackgroundEvent::ShowGui,
                Command::Settings => BackgroundEvent::OpenSettings,
                Command::Quit => BackgroundEvent::Quit,
                _ => return,
            };
            let _ = tx.send(ev);
            ctx.request_repaint();
        });
        match result {
            Ok(()) => {}
            Err(ListenError::AlreadyRunning) => crate::log::log("ui.sock: ein anderes Fenster läuft schon"),
            Err(ListenError::Io(e)) => crate::log::log(format!("ui.sock: {e}")),
        }
    }

    pub fn send_command(cmd: BgCommand) {
        match cmd {
            BgCommand::Notify { title, text } => crate::linux::desktop::notify(&title, &text),
            // The desktop owns the shortcuts (GlobalShortcuts portal, registered by
            // the daemon); recording a key in the window does not apply here.
            BgCommand::SetHotkeys(_)
            | BgCommand::StartRecording
            | BgCommand::CancelRecording
            | BgCommand::FinishRecording(_) => {}
        }
    }
}

#[cfg(windows)]
mod win {
    use super::{BackgroundEvent, BgCommand};
    use crate::settings::{Hotkey, HotkeyAction};
    use std::cell::RefCell;
    use std::ptr::null_mut;
    use std::sync::atomic::{AtomicIsize, Ordering};
    use std::sync::mpsc::Sender;
    use std::sync::Mutex;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
    };
    use windows_sys::Win32::UI::Shell::{
        Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIM_ADD,
        NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
        DestroyMenu, DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW, LoadIconW,
        PostMessageW, PostQuitMessage, RegisterClassW, SetForegroundWindow,
        TrackPopupMenu, TranslateMessage, IDI_APPLICATION,
        CreateIconIndirect, DestroyIcon, GetSystemMetrics, HICON, ICONINFO, SM_CXSMICON,
        MF_SEPARATOR, MF_STRING, MSG, TPM_BOTTOMALIGN, TPM_LEFTALIGN,
        TPM_RETURNCMD, WM_APP, WM_DESTROY, WM_HOTKEY,
        WM_LBUTTONUP, WM_RBUTTONUP, WNDCLASSW,
    };

    const WM_TRAY: u32 = WM_APP + 1;
    const WM_COMMAND_QUEUE: u32 = WM_APP + 2;

    const CMD_REGION: usize = 1001;
    const CMD_FULL: usize = 1002;
    const CMD_SHOW: usize = 1003;
    const CMD_SETTINGS: usize = 1004;
    const CMD_QUIT: usize = 1005;

    const TRAY_UID: u32 = 1;

    /// Hidden message window of the background thread (0 until created).
    static TRAY_HWND: AtomicIsize = AtomicIsize::new(0);
    /// Commands waiting to be processed by the background thread.
    static QUEUE: Mutex<Vec<BgCommand>> = Mutex::new(Vec::new());

    struct ThreadState {
        tx: Sender<BackgroundEvent>,
        ctx: eframe::egui::Context,
        bindings: Vec<(HotkeyAction, Hotkey)>,
        registered: Vec<HotkeyAction>,
        /// A new hotkey is being recorded; the global hotkeys are unregistered meanwhile.
        recording: bool,
    }

    thread_local! {
        static STATE: RefCell<Option<ThreadState>> = const { RefCell::new(None) };
    }

    /// Queue a command for the background thread and wake it up. Commands sent before
    /// the thread has created its window are processed right after it does.
    pub fn send_command(cmd: BgCommand) {
        if let Ok(mut q) = QUEUE.lock() {
            q.push(cmd);
        }
        let hwnd = TRAY_HWND.load(Ordering::Acquire);
        if hwnd != 0 {
            unsafe {
                PostMessageW(hwnd as HWND, WM_COMMAND_QUEUE, 0, 0);
            }
        }
    }

    pub fn start_background_listener(tx: Sender<BackgroundEvent>, ctx: eframe::egui::Context) {
        std::thread::spawn(move || run(tx, ctx));
    }

    fn emit(ev: BackgroundEvent) {
        let needs_ui = ev.wakes_window();
        STATE.with(|cell| {
            if let Some(st) = cell.borrow().as_ref() {
                let _ = st.tx.send(ev);
                st.ctx.request_repaint();
            }
        });
        if needs_ui {
            crate::window::wake();
        }
    }

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn copy_wide(dst: &mut [u16], s: &str) {
        let max_len = dst.len().saturating_sub(1);
        let mut n = 0;
        for (slot, ch) in dst.iter_mut().zip(s.encode_utf16()).take(max_len) {
            *slot = ch;
            n += 1;
        }
        for slot in dst.iter_mut().skip(n) {
            *slot = 0;
        }
    }

    fn region_label(bindings: &[(HotkeyAction, Hotkey)]) -> String {
        let labels: Vec<String> = bindings
            .iter()
            .filter(|(a, _)| matches!(a, HotkeyAction::Region | HotkeyAction::RegionAlt))
            .map(|(_, h)| h.label())
            .collect();
        labels.join(" / ")
    }

    fn base_nid(hwnd: HWND) -> NOTIFYICONDATAW {
        let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = TRAY_UID;
        nid
    }

    fn update_tray_tip(hwnd: HWND, bindings: &[(HotkeyAction, Hotkey)]) {
        let mut nid = base_nid(hwnd);
        nid.uFlags = NIF_TIP;
        let keys = region_label(bindings);
        let tip = if keys.is_empty() {
            crate::i18n::tr("Z-SHOT — Klick auf das Symbol für Bereich", "Z-SHOT — click the icon for a region").to_string()
        } else {
            crate::i18n::tr(format!("Z-SHOT — {keys} für Bereich"), format!("Z-SHOT — {keys} for a region"))
        };
        copy_wide(&mut nid.szTip, &tip);
        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &nid);
        }
    }

    fn show_balloon(hwnd: HWND, title: &str, text: &str) {
        let mut nid = base_nid(hwnd);
        nid.uFlags = NIF_INFO;
        nid.dwInfoFlags = NIIF_INFO;
        copy_wide(&mut nid.szInfoTitle, title);
        copy_wide(&mut nid.szInfo, text);
        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &nid);
        }
    }

    fn unregister_all(hwnd: HWND, st: &mut ThreadState) {
        for a in st.registered.drain(..) {
            unsafe {
                UnregisterHotKey(hwnd, a.id());
            }
        }
    }

    /// Registers `st.bindings` and reports per-action success to the app.
    fn register_all(hwnd: HWND, st: &mut ThreadState) -> Vec<(HotkeyAction, bool)> {
        unregister_all(hwnd, st);
        let mut results = Vec::new();
        for &(action, hk) in &st.bindings {
            let mut mods = MOD_NOREPEAT;
            if hk.ctrl {
                mods |= MOD_CONTROL;
            }
            if hk.shift {
                mods |= MOD_SHIFT;
            }
            if hk.alt {
                mods |= MOD_ALT;
            }
            if hk.win {
                mods |= MOD_WIN;
            }
            let ok = unsafe { RegisterHotKey(hwnd, action.id(), mods, hk.vk) } != 0;
            if ok {
                st.registered.push(action);
                crate::log::log(format!("Hotkey {} ({}): registriert", action.title(), hk.label()));
            } else {
                let err = std::io::Error::last_os_error();
                crate::log::log(format!(
                    "Hotkey {} ({}): NICHT registriert — {err}",
                    action.title(),
                    hk.label()
                ));
            }
            results.push((action, ok));
        }
        results
    }

    /// Ends recording (if active), re-registers the hotkeys and reports both results.
    fn finish_recording(hwnd: HWND, recorded: Option<Hotkey>) {
        crate::window::set_key_capture(false);
        let applied = STATE.with(|cell| {
            let mut guard = cell.borrow_mut();
            let st = guard.as_mut()?;
            if !st.recording {
                return None;
            }
            st.recording = false;
            Some(register_all(hwnd, st))
        });
        if let Some(applied) = applied {
            emit(BackgroundEvent::HotkeyRecorded(recorded));
            emit(BackgroundEvent::HotkeysApplied(applied));
        }
    }

    fn process_queue(hwnd: HWND) {
        let cmds: Vec<BgCommand> = QUEUE.lock().map(|mut q| q.drain(..).collect()).unwrap_or_default();
        for cmd in cmds {
            match cmd {
                BgCommand::SetHotkeys(bindings) => {
                    let applied = STATE.with(|cell| {
                        let mut guard = cell.borrow_mut();
                        let st = guard.as_mut()?;
                        st.bindings = bindings;
                        update_tray_tip(hwnd, &st.bindings);
                        // While recording, keep everything unregistered; the new
                        // bindings are registered once recording ends.
                        if st.recording {
                            None
                        } else {
                            Some(register_all(hwnd, st))
                        }
                    });
                    if let Some(applied) = applied {
                        emit(BackgroundEvent::HotkeysApplied(applied));
                    }
                }
                BgCommand::StartRecording => {
                    STATE.with(|cell| {
                        if let Some(st) = cell.borrow_mut().as_mut() {
                            if !st.recording {
                                // Unregistered first, so the combination (e.g. Print
                                // Screen) reaches the Z-SHOT window as a normal key.
                                unregister_all(hwnd, st);
                                st.recording = true;
                                crate::window::set_key_capture(true);
                            }
                        }
                    });
                }
                BgCommand::CancelRecording => finish_recording(hwnd, None),
                BgCommand::FinishRecording(recorded) => finish_recording(hwnd, recorded),
                BgCommand::Notify { title, text } => show_balloon(hwnd, &title, &text),
            }
        }
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_HOTKEY => {
                match HotkeyAction::from_id(wparam as i32) {
                    Some(HotkeyAction::Region | HotkeyAction::RegionAlt) => {
                        emit(BackgroundEvent::StartRegionCapture)
                    }
                    Some(HotkeyAction::Full) => emit(BackgroundEvent::StartFullCapture),
                    None => {}
                }
                0
            }
            WM_COMMAND_QUEUE => {
                process_queue(hwnd);
                0
            }
            WM_TRAY => {
                let ev = (lparam & 0xFFFF) as u32;
                if ev == WM_LBUTTONUP {
                    emit(BackgroundEvent::StartRegionCapture);
                } else if ev == WM_RBUTTONUP {
                    show_tray_menu(hwnd);
                }
                0
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    unsafe fn show_tray_menu(hwnd: HWND) {
        let hmenu = CreatePopupMenu();
        if hmenu.is_null() {
            return;
        }
        let keys = STATE.with(|cell| {
            cell.borrow().as_ref().map(|st| region_label(&st.bindings)).unwrap_or_default()
        });
        let region_text = if keys.is_empty() {
            crate::i18n::tr("Bereich aufnehmen", "Capture region").to_string()
        } else {
            crate::i18n::tr(format!("Bereich aufnehmen ({keys})"), format!("Capture region ({keys})"))
        };
        let s_region = to_wide(&region_text);
        let s_full = to_wide(crate::i18n::tr("Vollbild aufnehmen", "Capture fullscreen"));
        let s_show = to_wide(crate::i18n::tr("Oberfläche öffnen", "Open window"));
        let s_settings = to_wide(crate::i18n::tr("Einstellungen…", "Settings…"));
        let s_quit = to_wide(crate::i18n::tr("Beenden", "Quit"));
        AppendMenuW(hmenu, MF_STRING, CMD_REGION, s_region.as_ptr());
        AppendMenuW(hmenu, MF_STRING, CMD_FULL, s_full.as_ptr());
        AppendMenuW(hmenu, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(hmenu, MF_STRING, CMD_SHOW, s_show.as_ptr());
        AppendMenuW(hmenu, MF_STRING, CMD_SETTINGS, s_settings.as_ptr());
        AppendMenuW(hmenu, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(hmenu, MF_STRING, CMD_QUIT, s_quit.as_ptr());

        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(
            hmenu,
            TPM_RETURNCMD | TPM_BOTTOMALIGN | TPM_LEFTALIGN,
            pt.x,
            pt.y,
            0,
            hwnd,
            std::ptr::null(),
        ) as usize;
        DestroyMenu(hmenu);

        match cmd {
            CMD_REGION => emit(BackgroundEvent::StartRegionCapture),
            CMD_FULL => emit(BackgroundEvent::StartFullCapture),
            CMD_SHOW => emit(BackgroundEvent::ShowGui),
            CMD_SETTINGS => emit(BackgroundEvent::OpenSettings),
            CMD_QUIT => {
                emit(BackgroundEvent::Quit);
                PostQuitMessage(0);
            }
            _ => {}
        }
    }

    fn run(tx: Sender<BackgroundEvent>, ctx: eframe::egui::Context) {
        STATE.with(|cell| {
            *cell.borrow_mut() = Some(ThreadState {
                tx,
                ctx,
                bindings: Vec::new(),
                registered: Vec::new(),
                recording: false,
            });
        });

        unsafe {
            let hinstance = GetModuleHandleW(std::ptr::null());
            let class_name = to_wide("ZShotTrayWnd");

            let wc = WNDCLASSW {
                style: 0,
                lpfnWndProc: Some(wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinstance,
                hIcon: LoadIconW(null_mut(), IDI_APPLICATION),
                hCursor: null_mut(),
                hbrBackground: null_mut(),
                lpszMenuName: std::ptr::null(),
                lpszClassName: class_name.as_ptr(),
            };
            RegisterClassW(&wc);

            let hwnd = CreateWindowExW(
                0,
                class_name.as_ptr(),
                class_name.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                null_mut(),
                null_mut(),
                hinstance,
                null_mut(),
            );
            if hwnd.is_null() {
                return;
            }

            // System tray icon (Heroicons camera) with startup balloon notification
            let tray_icon = app_hicon();
            let mut nid = base_nid(hwnd);
            nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_INFO;
            nid.uCallbackMessage = WM_TRAY;
            nid.hIcon = if tray_icon.is_null() {
                LoadIconW(null_mut(), IDI_APPLICATION)
            } else {
                tray_icon
            };
            nid.dwInfoFlags = NIIF_INFO;
            copy_wide(&mut nid.szTip, "Z-SHOT");
            copy_wide(&mut nid.szInfoTitle, crate::i18n::tr("Z-SHOT läuft im Hintergrund", "Z-SHOT is running in the background"));
            copy_wide(
                &mut nid.szInfo,
                crate::i18n::tr(
                    "Tastenkürzel drücken oder auf das Symbol klicken, um einen Bereich aufzunehmen. Rechtsklick: Einstellungen.",
                    "Press the hotkey or click the icon to capture a region. Right-click: settings.",
                ),
            );
            Shell_NotifyIconW(NIM_ADD, &nid);

            TRAY_HWND.store(hwnd as isize, Ordering::Release);
            // Hotkeys arrive as a SetHotkeys command queued by the app at startup.
            process_queue(hwnd);

            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            TRAY_HWND.store(0, Ordering::Release);
            STATE.with(|cell| {
                if let Some(st) = cell.borrow_mut().as_mut() {
                    unregister_all(hwnd, st);
                }
            });
            Shell_NotifyIconW(NIM_DELETE, &nid);
            if !tray_icon.is_null() {
                DestroyIcon(tray_icon);
            }
            DestroyWindow(hwnd);
        }
    }

    /// Builds an HICON with alpha from the app icon at the system's small-icon size.
    fn app_hicon() -> HICON {
        use windows_sys::Win32::Graphics::Gdi::{
            CreateBitmap, CreateDIBSection, DeleteObject, GetDC, ReleaseDC, BITMAPINFO,
            BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        };
        unsafe {
            let size = GetSystemMetrics(SM_CXSMICON).clamp(16, 256);
            let rgba = crate::icons::app_icon_rgba(size as u32);

            let mut bmi: BITMAPINFO = std::mem::zeroed();
            bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = size;
            bmi.bmiHeader.biHeight = -size; // top-down
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = BI_RGB;

            let hdc = GetDC(null_mut());
            let mut bits: *mut core::ffi::c_void = null_mut();
            let color = CreateDIBSection(hdc, &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            ReleaseDC(null_mut(), hdc);
            if color.is_null() || bits.is_null() {
                return null_mut();
            }
            let dst = std::slice::from_raw_parts_mut(bits as *mut u8, rgba.len());
            for (d, s) in dst.chunks_exact_mut(4).zip(rgba.chunks_exact(4)) {
                d.copy_from_slice(&[s[2], s[1], s[0], s[3]]); // RGBA -> BGRA
            }

            // All-zero AND mask: transparency comes from the alpha channel.
            let row_bytes = ((size + 15) / 16 * 2) as usize;
            let mask_bits = vec![0u8; row_bytes * size as usize];
            let mask = CreateBitmap(size, size, 1, 1, mask_bits.as_ptr() as *const _);

            let info = ICONINFO {
                fIcon: 1,
                xHotspot: 0,
                yHotspot: 0,
                hbmMask: mask,
                hbmColor: color,
            };
            let icon = CreateIconIndirect(&info);
            DeleteObject(color);
            DeleteObject(mask);
            icon
        }
    }
}
