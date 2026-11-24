// Z-SHOT — Copyright (C) 2026 R. Trunk
// SPDX-License-Identifier: GPL-3.0-or-later

//! Win32 helpers for the main (egui) window.
//!
//! Rules that keep window state consistent between Win32, winit and egui:
//! - A hidden window always sits off-screen ([`OFFSCREEN`]); the app parks it there
//!   before hiding it. Waking it from the background thread ([`wake`]) therefore only
//!   needs to *show* it, never to move it, so it cannot race with the UI thread's own
//!   positioning (e.g. the region overlay on the primary monitor).
//! - The editor's place is remembered and restored in physical pixels
//!   ([`Placement`]). egui positions are in points, converted with the scale factor
//!   of the monitor the window is on *at that moment*, which puts the window in the
//!   wrong spot on setups with mixed scaling (e.g. 125 % + 100 %).

/// Off-screen position (physical pixels) where the window waits while hidden or
/// while it must keep receiving frames out of sight.
pub const OFFSCREEN: i32 = -32000;

/// Where the editor window was, in physical screen pixels. For a maximised window,
/// `x`/`y` is a point inside the monitor it was maximised on and `w`/`h` its restored
/// size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub maximized: bool,
}

#[cfg(windows)]
pub use imp::{
    cover_primary, handled_token, hide, is_visible, mark_handled, park_offscreen, placement,
    primary_size, restore, set_key_capture, set_ui_window, set_want_hidden, wake,
};

#[cfg(not(windows))]
mod fallback {
    use super::Placement;
    pub fn set_ui_window(_hwnd: isize) {}
    pub fn set_key_capture(_on: bool) {}
    pub fn wake() {}
    pub fn park_offscreen() {}
    pub fn is_visible() -> bool {
        false
    }
    pub fn placement() -> Option<Placement> {
        None
    }
    pub fn restore(_p: &Placement) {}
    pub fn primary_size() -> (i32, i32) {
        (1920, 1080)
    }
    pub fn cover_primary() {}
    pub fn hide() {}
    pub fn handled_token() -> u64 {
        0
    }
    pub fn mark_handled(_token: u64) {}
    pub fn set_want_hidden(_hidden: bool) {}
}
#[cfg(not(windows))]
pub use fallback::*;

#[cfg(windows)]
mod imp {
    use super::{Placement, OFFSCREEN};
    use std::ptr::null_mut;
    use crate::settings::Hotkey;
    use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, AtomicU64, Ordering};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyState, VK_CONTROL, VK_ESCAPE, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU,
        VK_PROCESSKEY, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_SNAPSHOT,
    };
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        WM_CHAR, WM_DEADCHAR, WM_KEYDOWN, WM_KEYUP, WM_SYSCHAR, WM_SYSDEADCHAR, WM_SYSKEYDOWN,
        WM_SYSKEYUP,
    };
    use windows_sys::Win32::Graphics::Gdi::{RedrawWindow, RDW_INTERNALPAINT, RDW_INVALIDATE};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN, GetWindowPlacement, GetWindowRect, IsIconic, IsWindowVisible, IsZoomed,
        SetWindowPlacement, SetWindowPos, ShowWindow, ShowWindowAsync, SWP_NOACTIVATE,
        SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SW_HIDE, SW_RESTORE, SW_SHOWNA,
        SW_SHOWNOACTIVATE, WINDOWPLACEMENT, WPF_RESTORETOMAXIMIZED,
    };

    /// The egui main window (0 until the app registered it).
    static UI_HWND: AtomicIsize = AtomicIsize::new(0);

    /// Registers the egui window. Must be called on the UI thread (the window's own
    /// thread), because it also installs the key-capture subclass there.
    pub fn set_ui_window(hwnd: isize) {
        UI_HWND.store(hwnd, Ordering::Release);
        unsafe {
            SetWindowSubclass(hwnd as HWND, Some(key_capture_proc), KEY_CAPTURE_SUBCLASS_ID, 0);
        }
    }

    // --- Hotkey recording ------------------------------------------------------
    //
    // A new hotkey is recorded from the key messages of the Z-SHOT window itself, while
    // it has focus: no system-wide keyboard hook. The global hotkeys are unregistered
    // during recording, so even Print Screen arrives here (as WM_KEYUP).

    const KEY_CAPTURE_SUBCLASS_ID: usize = 0x5A53;
    /// The next non-modifier key pressed in the window is the new hotkey.
    static KEY_CAPTURE: AtomicBool = AtomicBool::new(false);
    /// Key that was just recorded: its remaining messages (key-up, WM_CHAR) are
    /// swallowed so they don't reach egui as input.
    static SWALLOW_VK: AtomicU32 = AtomicU32::new(0);

    pub fn set_key_capture(on: bool) {
        KEY_CAPTURE.store(on, Ordering::Release);
    }

    fn is_modifier(vk: u32) -> bool {
        [
            VK_SHIFT, VK_LSHIFT, VK_RSHIFT, VK_CONTROL, VK_LCONTROL, VK_RCONTROL, VK_MENU,
            VK_LMENU, VK_RMENU, VK_LWIN, VK_RWIN,
        ]
        .iter()
        .any(|&m| m as u32 == vk)
    }

    /// Modifier state as of the message being processed (not a live keyboard poll).
    fn modifier_down(vk: u16) -> bool {
        unsafe { (GetKeyState(vk as i32) as u16 & 0x8000) != 0 }
    }

    unsafe extern "system" fn key_capture_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        let vk = wparam as u32;
        let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
        let is_char = matches!(msg, WM_CHAR | WM_SYSCHAR | WM_DEADCHAR | WM_SYSDEADCHAR);

        if KEY_CAPTURE.load(Ordering::Acquire) {
            // Print Screen only arrives as key-up.
            let snapshot_up = is_up && vk == VK_SNAPSHOT as u32;
            if (is_down || snapshot_up) && !is_modifier(vk) && vk != VK_PROCESSKEY as u32 {
                KEY_CAPTURE.store(false, Ordering::Release);
                let ctrl = modifier_down(VK_CONTROL);
                let shift = modifier_down(VK_SHIFT);
                let alt = modifier_down(VK_MENU);
                let win = modifier_down(VK_LWIN) || modifier_down(VK_RWIN);
                let recorded = if vk == VK_ESCAPE as u32 && !(ctrl || shift || alt || win) {
                    None
                } else {
                    Some(Hotkey { ctrl, shift, alt, win, vk })
                };
                if !snapshot_up {
                    SWALLOW_VK.store(vk, Ordering::Release);
                }
                crate::background::send_command(crate::background::BgCommand::FinishRecording(recorded));
                return 0;
            }
            if is_char {
                return 0;
            }
        }

        let swallow = SWALLOW_VK.load(Ordering::Acquire);
        if swallow != 0 {
            if is_char {
                return 0;
            }
            if is_up && vk == swallow {
                SWALLOW_VK.store(0, Ordering::Release);
                return 0;
            }
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }

    fn hwnd() -> Option<HWND> {
        let hwnd = UI_HWND.load(Ordering::Acquire) as HWND;
        (!hwnd.is_null()).then_some(hwnd)
    }

    fn move_to(hwnd: HWND, x: i32, y: i32) {
        unsafe {
            SetWindowPos(hwnd, null_mut(), x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
    }

    fn normal_placement(hwnd: HWND) -> Option<WINDOWPLACEMENT> {
        let mut wp: WINDOWPLACEMENT = unsafe { std::mem::zeroed() };
        wp.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
        (unsafe { GetWindowPlacement(hwnd, &mut wp) } != 0).then_some(wp)
    }

    /// Turns a maximised or minimised window into a normal, visible one at the
    /// off-screen spot (keeping its restored size), without it appearing on-screen.
    fn normalize_offscreen(hwnd: HWND) {
        if let Some(mut wp) = normal_placement(hwnd) {
            let r = &mut wp.rcNormalPosition;
            let (w, h) = (r.right - r.left, r.bottom - r.top);
            *r = RECT { left: OFFSCREEN, top: OFFSCREEN, right: OFFSCREEN + w, bottom: OFFSCREEN + h };
            wp.flags &= !WPF_RESTORETOMAXIMIZED;
            wp.showCmd = SW_SHOWNOACTIVATE as u32;
            unsafe {
                SetWindowPlacement(hwnd, &wp);
            }
        }
        // SetWindowPlacement pulls a restored window back onto a monitor; this
        // SetWindowPos does not.
        move_to(hwnd, OFFSCREEN, OFFSCREEN);
    }

    /// Number of wake-ups requested (by [`wake`]) and handled (by [`mark_handled`]).
    static WAKES_REQUESTED: AtomicU64 = AtomicU64::new(0);
    static WAKES_HANDLED: AtomicU64 = AtomicU64::new(0);
    /// The UI wants the window hidden (it is in background mode).
    static WANT_HIDDEN: AtomicBool = AtomicBool::new(true);

    /// UI thread, at the start of a frame *before* draining the event channel:
    /// returns a token for [`mark_handled`]. Events are sent before the counter is
    /// bumped, so everything counted up to this token is in the channel already.
    pub fn handled_token() -> u64 {
        WAKES_REQUESTED.load(Ordering::Acquire)
    }

    /// UI thread: all events up to `token` (see [`handled_token`]) are processed.
    pub fn mark_handled(token: u64) {
        WAKES_HANDLED.fetch_max(token, Ordering::AcqRel);
    }

    /// UI thread: whether the window should be hidden (background mode).
    pub fn set_want_hidden(hidden: bool) {
        WANT_HIDDEN.store(hidden, Ordering::Release);
    }

    /// Called on the tray/hotkey thread after an event was sent to the UI.
    ///
    /// Windows sends no WM_PAINT to hidden or minimised windows, and not reliably to
    /// windows lying completely off-screen either, so eframe may never run
    /// `update()` for the event. A short-lived helper thread therefore keeps the
    /// window shown (off-screen) and invalidated until the UI reports the event as
    /// handled. Neither the helper nor the tray thread ever waits for the UI thread:
    /// the show request is posted (ShowWindowAsync), not sent.
    ///
    /// Afterwards the helper re-hides the window if the UI wants it hidden but it is
    /// visible: frames can run while hidden, so a show requested here may land after
    /// the UI already handled the event and hid the window again (and eframe shows
    /// the window once after its very first frame).
    pub fn wake() {
        let Some(hwnd) = hwnd() else {
            crate::log::log("Aufwecken nicht möglich: Fenster-Handle fehlt");
            return;
        };
        let token = WAKES_REQUESTED.fetch_add(1, Ordering::AcqRel) + 1;
        let hwnd = hwnd as isize;
        std::thread::spawn(move || wake_until_handled(hwnd as HWND, token));
    }

    fn wake_until_handled(hwnd: HWND, token: u64) {
        let handled = || WAKES_HANDLED.load(Ordering::Acquire) >= token;
        let started = std::time::Instant::now();
        let mut last_show: Option<std::time::Instant> = None;
        while !handled() {
            if started.elapsed() > std::time::Duration::from_secs(15) {
                crate::log::log("Aufwecken: UI hat das Ereignis nach 15 s nicht verarbeitet");
                return;
            }
            unsafe {
                if IsIconic(hwnd) != 0 {
                    // Minimised editor: restoring would bring it back on-screen (and
                    // into the screenshot), so restore it to an off-screen spot.
                    normalize_offscreen(hwnd);
                    crate::log::log("Fenster war minimiert: außerhalb des Bildschirms wiederhergestellt");
                } else if IsWindowVisible(hwnd) == 0
                    && last_show.is_none_or(|t| t.elapsed() > std::time::Duration::from_millis(250))
                {
                    // Only show, don't move: a hidden window is always off-screen.
                    // Posted, so this thread never blocks on a busy UI thread; repeated
                    // now and then in case the UI hid the window again in the meantime.
                    ShowWindowAsync(hwnd, SW_SHOWNA);
                    if last_show.is_none() {
                        crate::log::log("Fenster war versteckt: außerhalb des Bildschirms eingeblendet");
                    }
                    last_show = Some(std::time::Instant::now());
                }
                RedrawWindow(hwnd, std::ptr::null(), null_mut(), RDW_INVALIDATE | RDW_INTERNALPAINT);
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
        unsafe {
            if WANT_HIDDEN.load(Ordering::Acquire) && IsWindowVisible(hwnd) != 0 {
                ShowWindowAsync(hwnd, SW_HIDE);
            }
        }
    }

    /// UI thread: moves the window off-screen but keeps it visible, so eframe keeps
    /// running frames during the capture delay. The short hide/show hands keyboard
    /// focus and activation to the window below, so the captured app does not show
    /// an inactive title bar. Done via Win32 (not ViewportCommand) so it happens
    /// before the screen is captured, and winit's visibility state stays correct.
    ///
    /// A maximised or minimised window is first turned into a normal one (its
    /// remembered [`Placement`] keeps the maximised state): a maximised window lying
    /// outside every monitor gets no WM_PAINT after being shown, so the app would
    /// never see the next hotkey.
    pub fn park_offscreen() {
        let Some(hwnd) = hwnd() else { return };
        unsafe {
            let was_visible = IsWindowVisible(hwnd) != 0;
            if IsZoomed(hwnd) != 0 || IsIconic(hwnd) != 0 {
                normalize_offscreen(hwnd);
            } else {
                move_to(hwnd, OFFSCREEN, OFFSCREEN);
            }
            // Frames can still run right after hiding; never show a hidden window here.
            if was_visible {
                ShowWindow(hwnd, SW_HIDE);
                ShowWindow(hwnd, SW_SHOWNA);
            }
        }
    }

    pub fn is_visible() -> bool {
        hwnd().is_some_and(|h| unsafe { IsWindowVisible(h) } != 0)
    }

    /// UI thread: hides the window right away (it stays parked where it is).
    pub fn hide() {
        if let Some(hwnd) = hwnd() {
            unsafe {
                ShowWindow(hwnd, SW_HIDE);
            }
        }
    }

    /// The window's current place, or `None` while it is minimised or off-screen.
    pub fn placement() -> Option<Placement> {
        let hwnd = hwnd()?;
        unsafe {
            if IsIconic(hwnd) != 0 {
                return None;
            }
            let mut r: RECT = std::mem::zeroed();
            if GetWindowRect(hwnd, &mut r) == 0 || r.left < -10000 || r.top < -10000 {
                return None;
            }
            if IsZoomed(hwnd) != 0 {
                // Remember the monitor (a point inside it) and the restored size.
                let n = normal_placement(hwnd)?.rcNormalPosition;
                Some(Placement {
                    x: r.left + 16,
                    y: r.top + 16,
                    w: n.right - n.left,
                    h: n.bottom - n.top,
                    maximized: true,
                })
            } else {
                Some(Placement {
                    x: r.left,
                    y: r.top,
                    w: r.right - r.left,
                    h: r.bottom - r.top,
                    maximized: false,
                })
            }
        }
    }

    /// UI thread: puts the window back to `p` (the caller re-maximises it if needed).
    /// Moves first and sizes second: moving onto a monitor with another scaling
    /// triggers WM_DPICHANGED, and winit then rescales the window; setting the size
    /// afterwards gives the exact remembered size.
    ///
    /// Synchronous on purpose: winit's own positioning (set_outer_position,
    /// fullscreen) is posted with SWP_ASYNCWINDOWPOS and would run in an order we
    /// don't control.
    pub fn restore(p: &Placement) {
        let Some(hwnd) = hwnd() else { return };
        unsafe {
            if IsZoomed(hwnd) != 0 {
                ShowWindow(hwnd, SW_RESTORE);
            }
            move_to(hwnd, p.x, p.y);
            SetWindowPos(hwnd, null_mut(), 0, 0, p.w, p.h, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
    }

    /// Size of the primary monitor in physical pixels (it starts at 0,0).
    pub fn primary_size() -> (i32, i32) {
        unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) }
    }

    /// UI thread: makes the window cover the primary monitor exactly (region
    /// overlay). The app removes the decorations and raises it via winit; those
    /// style changes keep the window rect, so their order does not matter.
    pub fn cover_primary() {
        let (w, h) = primary_size();
        restore(&Placement { x: 0, y: 0, w, h, maximized: false });
    }
}
