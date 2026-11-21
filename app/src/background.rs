use std::sync::mpsc::Sender;

/// Events sent from the background hotkey / system-tray thread to the egui app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundEvent {
    StartRegionCapture,
    StartFullCapture,
    ShowGui,
    Quit,
}

#[cfg(windows)]
pub fn start_background_listener(
    tx: Sender<BackgroundEvent>,
    ctx: eframe::egui::Context,
) {
    std::thread::spawn(move || {
        run_win32_tray_and_hotkeys(tx, ctx);
    });
}

#[cfg(not(windows))]
pub fn start_background_listener(
    _tx: Sender<BackgroundEvent>,
    _ctx: eframe::egui::Context,
) {}

#[cfg(windows)]
fn run_win32_tray_and_hotkeys(tx: Sender<BackgroundEvent>, ctx: eframe::egui::Context) {
    use std::cell::RefCell;
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        RegisterHotKey, UnregisterHotKey, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, VK_SNAPSHOT,
    };
    use windows_sys::Win32::UI::Shell::{
        Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIM_ADD,
        NIM_DELETE, NOTIFYICONDATAW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
        DispatchMessageW, GetCursorPos, GetMessageW, LoadIconW, PostQuitMessage, RegisterClassW,
        SetForegroundWindow, TrackPopupMenu, TranslateMessage, IDI_APPLICATION, MF_SEPARATOR,
        MF_STRING, MSG, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RETURNCMD, WM_APP, WM_DESTROY,
        WM_HOTKEY, WM_LBUTTONUP, WM_RBUTTONUP, WNDCLASSW,
    };

    const WM_TRAY: u32 = WM_APP + 1;
    const HOTKEY_PRINTSCREEN: i32 = 1;
    const HOTKEY_CTRL_SHIFT_S: i32 = 2;

    const CMD_REGION: usize = 1001;
    const CMD_FULL: usize = 1002;
    const CMD_SHOW: usize = 1003;
    const CMD_QUIT: usize = 1004;

    thread_local! {
        static DISPATCH: RefCell<Option<(Sender<BackgroundEvent>, eframe::egui::Context)>> = const { RefCell::new(None) };
    }

    fn emit(ev: BackgroundEvent) {
        DISPATCH.with(|cell| {
            if let Some((tx, ctx)) = cell.borrow().as_ref() {
                let _ = tx.send(ev);
                ctx.request_repaint();
            }
        });
    }

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn copy_wide(dst: &mut [u16], s: &str) {
        let max_len = dst.len().saturating_sub(1);
        let mut iter = s.encode_utf16();
        for slot in dst.iter_mut().take(max_len) {
            if let Some(ch) = iter.next() {
                *slot = ch;
            } else {
                *slot = 0;
                break;
            }
        }
        if let Some(last) = dst.last_mut() {
            *last = 0;
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
                let id = wparam as i32;
                if id == HOTKEY_PRINTSCREEN || id == HOTKEY_CTRL_SHIFT_S {
                    emit(BackgroundEvent::StartRegionCapture);
                }
                0
            }
            WM_TRAY => {
                let ev = (lparam & 0xFFFF) as u32;
                if ev == WM_LBUTTONUP {
                    emit(BackgroundEvent::StartRegionCapture);
                } else if ev == WM_RBUTTONUP {
                    let hmenu = CreatePopupMenu();
                    if !hmenu.is_null() {
                        let s_region = to_wide("Bereich aufnehmen (Druck / Strg+Umschalt+S)");
                        let s_full = to_wide("Vollbild aufnehmen");
                        let s_show = to_wide("Oberfläche öffnen");
                        let s_quit = to_wide("Beenden");
                        AppendMenuW(hmenu, MF_STRING, CMD_REGION, s_region.as_ptr());
                        AppendMenuW(hmenu, MF_STRING, CMD_FULL, s_full.as_ptr());
                        AppendMenuW(hmenu, MF_SEPARATOR, 0, std::ptr::null());
                        AppendMenuW(hmenu, MF_STRING, CMD_SHOW, s_show.as_ptr());
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
                            CMD_QUIT => {
                                emit(BackgroundEvent::Quit);
                                PostQuitMessage(0);
                            }
                            _ => {}
                        }
                    }
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

    DISPATCH.with(|cell| {
        *cell.borrow_mut() = Some((tx, ctx));
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

        // Register global hotkeys: Print Screen and Ctrl+Shift+S
        let _ = RegisterHotKey(hwnd, HOTKEY_PRINTSCREEN, MOD_NOREPEAT, VK_SNAPSHOT as u32);
        let _ = RegisterHotKey(
            hwnd,
            HOTKEY_CTRL_SHIFT_S,
            MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT,
            b'S' as u32,
        );

        // Add Windows system tray icon with startup balloon notification
        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_INFO;
        nid.uCallbackMessage = WM_TRAY;
        nid.hIcon = LoadIconW(null_mut(), IDI_APPLICATION);
        nid.dwInfoFlags = NIIF_INFO;
        copy_wide(&mut nid.szTip, "Z-SHOT — Druck oder Strg+Umschalt+S für Bereich");
        copy_wide(&mut nid.szInfoTitle, "Z-SHOT läuft im Hintergrund");
        copy_wide(
            &mut nid.szInfo,
            "Drücke Druck oder Strg+Umschalt+S (oder klicke auf das Symbol), um einen Bereich aufzunehmen.",
        );
        Shell_NotifyIconW(NIM_ADD, &nid);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        Shell_NotifyIconW(NIM_DELETE, &nid);
        let _ = UnregisterHotKey(hwnd, HOTKEY_PRINTSCREEN);
        let _ = UnregisterHotKey(hwnd, HOTKEY_CTRL_SHIFT_S);
        DestroyWindow(hwnd);
    }
}
