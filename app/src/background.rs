// Z-SHOT — Copyright (C) 2026 R. Trunk
// SPDX-License-Identifier: GPL-3.0-or-later

//! Background thread: system-tray icon, global hotkeys and hotkey recording.
//!
//! The thread runs a Win32 message loop on a hidden window. The egui app talks to it
//! through [`send_command`] (queued + a posted wake-up message) and receives
//! [`BackgroundEvent`]s over an mpsc channel.

use crate::settings::{Hotkey, HotkeyAction};
#[cfg(not(windows))]
use std::sync::mpsc::Sender;

/// Events sent from the background hotkey / system-tray thread to the egui app.
#[derive(Debug, Clone, PartialEq, Eq)]
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
pub enum BgCommand {
    SetHotkeys(Vec<(HotkeyAction, Hotkey)>),
    /// Unregister hotkeys and capture the next key combination system-wide.
    StartRecording,
    CancelRecording,
    Notify { title: String, text: String },
}

#[cfg(windows)]
pub use win::{send_command, start_background_listener};

#[cfg(not(windows))]
pub fn start_background_listener(_tx: Sender<BackgroundEvent>, _ctx: eframe::egui::Context) {}

#[cfg(not(windows))]
pub fn send_command(_cmd: BgCommand) {}

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
        GetAsyncKeyState, RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
        MOD_SHIFT, MOD_WIN, VK_CONTROL, VK_ESCAPE, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN,
        VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_SNAPSHOT,
    };
    use windows_sys::Win32::UI::Shell::{
        Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIM_ADD,
        NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CallNextHookEx, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
        DestroyMenu, DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW, LoadIconW,
        PostMessageW, PostQuitMessage, RegisterClassW, SetForegroundWindow, SetWindowsHookExW,
        TrackPopupMenu, TranslateMessage, UnhookWindowsHookEx, HHOOK, IDI_APPLICATION,
        CreateIconIndirect, DestroyIcon, GetSystemMetrics, HICON, ICONINFO, SM_CXSMICON,
        KBDLLHOOKSTRUCT, MF_SEPARATOR, MF_STRING, MSG, TPM_BOTTOMALIGN, TPM_LEFTALIGN,
        TPM_RETURNCMD, WH_KEYBOARD_LL, WM_APP, WM_DESTROY, WM_HOTKEY, WM_KEYDOWN, WM_KEYUP,
        WM_LBUTTONUP, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP, WNDCLASSW,
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
        hook: HHOOK,
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
            "Z-SHOT — Klick auf das Symbol für Bereich".to_string()
        } else {
            format!("Z-SHOT — {keys} für Bereich")
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

    fn is_modifier(vk: u32) -> bool {
        [
            VK_SHIFT, VK_LSHIFT, VK_RSHIFT, VK_CONTROL, VK_LCONTROL, VK_RCONTROL, VK_MENU,
            VK_LMENU, VK_RMENU, VK_LWIN, VK_RWIN,
        ]
        .iter()
        .any(|&m| m as u32 == vk)
    }

    fn key_down(vk: u16) -> bool {
        unsafe { (GetAsyncKeyState(vk as i32) as u16 & 0x8000) != 0 }
    }

    /// Ends recording (if active), re-registers the hotkeys and reports both results.
    fn finish_recording(hwnd: HWND, recorded: Option<Hotkey>) {
        let applied = STATE.with(|cell| {
            let mut guard = cell.borrow_mut();
            let st = guard.as_mut()?;
            if st.hook.is_null() {
                return None;
            }
            unsafe {
                UnhookWindowsHookEx(st.hook);
            }
            st.hook = null_mut();
            Some(register_all(hwnd, st))
        });
        if let Some(applied) = applied {
            emit(BackgroundEvent::HotkeyRecorded(recorded));
            emit(BackgroundEvent::HotkeysApplied(applied));
        }
    }

    /// Low-level keyboard hook, only installed while recording a new hotkey. It sees
    /// every key system-wide (including Print Screen, which normal windows don't get
    /// as WM_KEYDOWN) and swallows the recorded key so it doesn't reach other apps.
    unsafe extern "system" fn record_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            let info = &*(lparam as *const KBDLLHOOKSTRUCT);
            let vk = info.vkCode;
            let msg = wparam as u32;
            let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
            // Print Screen sometimes only arrives as key-up.
            let is_snapshot_up = (msg == WM_KEYUP || msg == WM_SYSKEYUP) && vk == VK_SNAPSHOT as u32;
            if (is_down || is_snapshot_up) && !is_modifier(vk) {
                let hwnd = TRAY_HWND.load(Ordering::Acquire) as HWND;
                let ctrl = key_down(VK_CONTROL);
                let shift = key_down(VK_SHIFT);
                let alt = key_down(VK_MENU);
                let win = key_down(VK_LWIN) || key_down(VK_RWIN);
                let recorded = if vk == VK_ESCAPE as u32 && !(ctrl || shift || alt || win) {
                    None
                } else {
                    Some(Hotkey { ctrl, shift, alt, win, vk })
                };
                finish_recording(hwnd, recorded);
                return 1;
            }
        }
        CallNextHookEx(null_mut(), code, wparam, lparam)
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
                        if st.hook.is_null() {
                            Some(register_all(hwnd, st))
                        } else {
                            None
                        }
                    });
                    if let Some(applied) = applied {
                        emit(BackgroundEvent::HotkeysApplied(applied));
                    }
                }
                BgCommand::StartRecording => {
                    STATE.with(|cell| {
                        if let Some(st) = cell.borrow_mut().as_mut() {
                            if st.hook.is_null() {
                                unregister_all(hwnd, st);
                                st.hook = unsafe {
                                    SetWindowsHookExW(
                                        WH_KEYBOARD_LL,
                                        Some(record_hook),
                                        GetModuleHandleW(std::ptr::null()),
                                        0,
                                    )
                                };
                            }
                        }
                    });
                }
                BgCommand::CancelRecording => finish_recording(hwnd, None),
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
            "Bereich aufnehmen".to_string()
        } else {
            format!("Bereich aufnehmen ({keys})")
        };
        let s_region = to_wide(&region_text);
        let s_full = to_wide("Vollbild aufnehmen");
        let s_show = to_wide("Oberfläche öffnen");
        let s_settings = to_wide("Einstellungen…");
        let s_quit = to_wide("Beenden");
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
                hook: null_mut(),
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
            copy_wide(&mut nid.szInfoTitle, "Z-SHOT läuft im Hintergrund");
            copy_wide(
                &mut nid.szInfo,
                "Tastenkürzel drücken oder auf das Symbol klicken, um einen Bereich aufzunehmen. Rechtsklick: Einstellungen.",
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
                    if !st.hook.is_null() {
                        UnhookWindowsHookEx(st.hook);
                        st.hook = null_mut();
                    }
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
