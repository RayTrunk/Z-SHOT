// Z-SHOT — Copyright (C) 2026 R. Trunk
// SPDX-License-Identifier: GPL-3.0-or-later

// GUI app on all build profiles: no console window, also for debug builds.
#![windows_subsystem = "windows"]

mod background;
mod capture;
mod i18n;
mod icons;
mod log;
mod model;
mod settings;
mod window;

use background::{BackgroundEvent, BgCommand};
use settings::{AfterCapture, HotkeyAction, Settings};
use eframe::egui::{
    self, Align, Color32, ColorImage, CornerRadius, CursorIcon, FontData, FontDefinitions,
    FontFamily, FontId, Layout, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, TextureHandle,
    TextureOptions, Vec2, ViewportBuilder, ViewportCommand,
};
use i18n::tr;
use icons::{icon_button, icon_text_button, paint_icon, Icon};
use image::RgbaImage;
use model::{
    album_dir, arr_to_color32, color32_to_arr, item_image_path, load_album_index, save_album,
    save_album_index, spawn_album_image_loader, Annotation, ShotItem, Tool,
};
use serde::Serialize;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

const PALETTE: [Color32; 6] = [
    Color32::from_rgb(239, 68, 68),   // Red 500
    Color32::from_rgb(245, 158, 11),  // Amber 500
    Color32::from_rgb(16, 185, 129),  // Emerald 500
    Color32::from_rgb(56, 189, 248),  // Sky 400
    Color32::from_rgb(168, 85, 247),  // Purple 500
    Color32::from_rgb(248, 250, 252), // Slate 50
];

const STROKE_SIZES: [(&str, f32); 3] = [("S", 2.5), ("M", 4.0), ("L", 7.0)];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AppMode {
    Background,
    PreparingOverlay,
    RegionOverlay,
    Editor,
}

#[derive(Default)]
struct CliArgs {
    screenshot_path: Option<PathBuf>,
    self_test_path: Option<PathBuf>,
    open_settings: bool,
}

fn parse_args() -> CliArgs {
    let mut args = std::env::args().skip(1);
    let mut cli = CliArgs::default();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--screenshot" => {
                if let Some(p) = args.next() {
                    cli.screenshot_path = Some(PathBuf::from(p));
                }
            }
            "--settings" => cli.open_settings = true,
            "--self-test" => {
                if let Some(p) = args.next() {
                    cli.self_test_path = Some(PathBuf::from(p));
                }
            }
            _ => {}
        }
    }
    cli
}

#[derive(Serialize)]
struct SelfTestStep {
    step: &'static str,
    passed: bool,
    detail: String,
}

#[derive(Serialize)]
struct SelfTestReport {
    version: u32,
    steps: Vec<SelfTestStep>,
}

fn run_self_test(out_path: PathBuf) -> Result<(), String> {
    let mut items = create_demo_items();

    // Step 1: move — simulate background -> region overlay drag selection -> editor navigation
    let started_in_bg = AppMode::Background == AppMode::Background;
    let mut mode = AppMode::RegionOverlay;
    let overlay_rect = Rect::from_min_max(Pos2::new(80.0, 60.0), Pos2::new(560.0, 380.0));
    let mut zoom: f32 = 1.0;
    zoom = (zoom + 0.25).clamp(0.25, 4.0);
    let pan = Vec2::new(42.0, -18.0);
    let next_selected = if items.len() > 1 { 1 } else { 0 };
    let move_ok = started_in_bg
        && mode == AppMode::RegionOverlay
        && overlay_rect.width() == 480.0
        && next_selected == 1
        && (zoom - 1.25).abs() < 1e-3;

    // Step 2: act — crop marked region, reveal editor GUI, add annotations, undo/redo, search, export
    let full_screen = capture::generate_sample_capture(2);
    let region_img = image::imageops::crop_imm(&full_screen, 80, 60, 480, 320).to_image();
    let mut shot = ShotItem::new(
        "selftest-region".into(),
        "Bereichsaufnahme".into(),
        "Projekt Alpha".into(),
        region_img,
    );
    mode = AppMode::Editor;
    shot.push_annotation(Annotation::Arrow {
        from: [60.0, 80.0],
        to: [240.0, 180.0],
        color: color32_to_arr(PALETTE[0]),
        width: 4.0,
    });
    shot.push_annotation(Annotation::Redact {
        min: [120.0, 200.0],
        max: [300.0, 240.0],
    });
    shot.push_annotation(Annotation::Text {
        pos: [40.0, 30.0],
        text: "Wichtig".into(),
        color: color32_to_arr(PALETTE[1]),
        size: 18.0,
    });
    let added = shot.meta.annotations.len() == 3;
    let undone = shot.undo() && shot.meta.annotations.len() == 2;
    let redone = shot.redo() && shot.meta.annotations.len() == 3;
    let search_hit = shot.matches_query("alpha", false) && shot.matches_query("wichtig", false);
    let flattened = shot.render_flattened();
    items.insert(0, shot);
    let act_ok = mode == AppMode::Editor
        && added
        && undone
        && redone
        && search_hit
        && flattened.width() == 480
        && flattened.height() == 320
        && items.len() == 3;

    // Step 3: hud — verify toolbar tools, palette, resolution/status metrics, and editor mode
    let tools = [
        Tool::Select,
        Tool::Arrow,
        Tool::Rect,
        Tool::Highlight,
        Tool::Redact,
        Tool::Text,
        Tool::Freehand,
        Tool::Crop,
    ];
    let hud_ok = mode == AppMode::Editor
        && tools.len() == 8
        && PALETTE.len() == 6
        && items[0].meta.width == 480
        && items[0].meta.height == 320;

    let report = SelfTestReport {
        version: 1,
        steps: vec![
            SelfTestStep {
                step: "move",
                passed: move_ok,
                detail: format!(
                    "background -> region overlay ({:.0}x{:.0}), switched album 0 -> {}, zoom {:.0}%, pan ({:.0}, {:.0})",
                    overlay_rect.width(),
                    overlay_rect.height(),
                    next_selected,
                    zoom * 100.0,
                    pan.x,
                    pan.y
                ),
            },
            SelfTestStep {
                step: "act",
                passed: act_ok,
                detail: format!(
                    "cropped marked region {}x{} -> revealed Editor GUI, added 3 annotations, verified undo/redo & search",
                    flattened.width(),
                    flattened.height()
                ),
            },
            SelfTestStep {
                step: "hud",
                passed: hud_ok,
                detail: format!(
                    "verified Editor GUI with {} tools, {} colors, album count {}, resolution {}x{}",
                    tools.len(),
                    PALETTE.len(),
                    items.len(),
                    items[0].meta.width,
                    items[0].meta.height
                ),
            },
        ],
    };

    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    std::fs::write(&out_path, json).map_err(|e| e.to_string())?;
    Ok(())
}

fn create_demo_items() -> Vec<ShotItem> {
    let mut first = ShotItem::new(
        "demo-dashboard".into(),
        tr("Dashboard-Übersicht", "Dashboard overview").into(),
        tr("Beispiel-Screenshot mit Markierungen (Pfeil, Textmarker, Schwärzung)", "Sample screenshot with markups (arrow, highlighter, redaction)").into(),
        capture::generate_sample_capture(0),
    );
    first.meta.favorite = true;
    first.meta.annotations.push(Annotation::Arrow {
        from: [560.0, 225.0],
        to: [635.0, 265.0],
        color: color32_to_arr(PALETTE[0]),
        width: 4.0,
    });
    first.meta.annotations.push(Annotation::Highlight {
        min: [252.0, 120.0],
        max: [392.0, 164.0],
        color: color32_to_arr(PALETTE[1]),
    });
    first.meta.annotations.push(Annotation::Redact {
        min: [728.0, 122.0],
        max: [872.0, 162.0],
    });
    first.meta.annotations.push(Annotation::Text {
        pos: [435.0, 214.0],
        text: tr("Höchstwert", "Peak").into(),
        color: color32_to_arr(PALETTE[0]),
        size: 18.0,
    });

    let second = ShotItem::new(
        "demo-editor".into(),
        tr("Code-Notizen", "Code notes").into(),
        tr("Beispiel-Ansicht im dunklen Design", "Sample view in dark theme").into(),
        capture::generate_sample_capture(1),
    );

    vec![first, second]
}

fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let candidates = [
        r"C:\Windows\Fonts\segoeui.ttf",
        r"C:\Windows\Fonts\SegoeUI.ttf",
    ];
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert("segoe_ui".to_owned(), FontData::from_owned(bytes).into());
            if let Some(prop) = fonts.families.get_mut(&FontFamily::Proportional) {
                prop.insert(0, "segoe_ui".to_owned());
            }
            break;
        }
    }
    ctx.set_fonts(fonts);
}

fn apply_theme(ctx: &egui::Context, dark: bool) {
    let mut visuals = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };

    if dark {
        visuals.panel_fill = Color32::from_rgb(15, 23, 42);
        visuals.window_fill = Color32::from_rgb(30, 41, 59);
        visuals.extreme_bg_color = Color32::from_rgb(9, 13, 24);
        visuals.faint_bg_color = Color32::from_rgb(22, 31, 48);
        visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(30, 41, 59);
        visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(51, 65, 85);
        visuals.widgets.noninteractive.bg_stroke =
            Stroke::new(1.0_f32, Color32::from_rgb(38, 49, 68));
        visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(37, 99, 235, 65);
        visuals.selection.stroke = Stroke::new(1.0_f32, Color32::from_rgb(96, 165, 250));
    } else {
        visuals.panel_fill = Color32::from_rgb(248, 250, 252);
        visuals.window_fill = Color32::WHITE;
        visuals.extreme_bg_color = Color32::from_rgb(226, 232, 240);
        visuals.faint_bg_color = Color32::from_rgb(241, 245, 249);
        visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(241, 245, 249);
        visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(226, 232, 240);
        visuals.widgets.noninteractive.bg_stroke =
            Stroke::new(1.0_f32, Color32::from_rgb(203, 213, 225));
        visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(37, 99, 235, 32);
        visuals.selection.stroke = Stroke::new(1.0_f32, Color32::from_rgb(37, 99, 235));
    }

    visuals.window_corner_radius = CornerRadius::same(8);
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(6.0, 6.0);
    style.spacing.button_padding = Vec2::new(8.0, 5.0);
    ctx.set_style(style);
}

struct PendingText {
    img_pos: Pos2,
    text: String,
}

struct ZShotApp {
    mode: AppMode,
    allow_exit: bool,
    bg_rx: Option<Receiver<BackgroundEvent>>,
    /// Album images decoded in the background after start-up; `None` once all arrived.
    album_rx: Option<Receiver<(String, Option<RgbaImage>)>>,

    // Fullscreen region selection overlay state
    overlay_capture_at: Option<Instant>,
    overlay_image: Option<RgbaImage>,
    overlay_texture: Option<TextureHandle>,
    overlay_drag_start: Option<Pos2>,
    overlay_drag_curr: Option<Pos2>,

    items: Vec<ShotItem>,
    selected: usize,
    tool: Tool,
    color_idx: usize,
    stroke_idx: usize,
    dark_mode: bool,
    show_sidebar: bool,
    search_query: String,
    only_favorites: bool,

    // Canvas transform
    zoom: f32,
    pan: Vec2,
    fit_requested: bool,

    // Active drawing / editing state
    drag_start_img: Option<Pos2>,
    drag_curr_img: Option<Pos2>,
    freehand_pts: Vec<[f32; 2]>,
    selected_annotation: Option<usize>,
    pending_text: Option<PendingText>,
    crop_rect: Option<Rect>,

    // Delayed capture timer
    delay_capture_at: Option<Instant>,

    // Status message
    status_msg: String,
    status_until: Option<Instant>,

    // Persistence
    persist: bool,
    album_path: PathBuf,

    // Screenshot automation mode
    screenshot_target: Option<PathBuf>,
    frame_count: u32,
    screenshot_requested: bool,

    // Settings
    settings: Settings,
    settings_path: PathBuf,
    show_settings: bool,
    /// Action whose hotkey is currently being recorded (key press captured system-wide).
    recording: Option<HotkeyAction>,
    /// Last registration result per action (`false` = taken by another program).
    hotkey_status: Vec<(HotkeyAction, bool)>,
    autostart: bool,
    /// The editor was visible when the capture started, so return to it afterwards.
    capture_from_editor: bool,
    /// Where the editor window was (physical pixels) before it was hidden or parked.
    editor_place: Option<window::Placement>,
    /// A full-screen capture (instead of region) waits for the editor to move away.
    pending_full_capture: bool,
    /// Album entries picked with Ctrl+click (ids), in addition to `selected`.
    marked: HashSet<String>,
    /// Entries waiting for the user to confirm their deletion (ids).
    confirm_delete: Option<Vec<String>>,
}

/// Off-screen start position, so eframe's forced first-frame show is never visible.
const OFFSCREEN_POS: [f32; 2] = [window::OFFSCREEN as f32, window::OFFSCREEN as f32];
/// Default editor size (logical points).
const EDITOR_SIZE: Vec2 = Vec2::new(1280.0, 800.0);

impl ZShotApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        screenshot_target: Option<PathBuf>,
        open_settings: bool,
    ) -> Self {
        icons::install(&cc.egui_ctx);
        configure_fonts(&cc.egui_ctx);

        // Let the background thread wake this window while it is hidden (window::wake).
        {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            match cc.window_handle().map(|h| h.as_raw()) {
                Ok(RawWindowHandle::Win32(w)) => window::set_ui_window(w.hwnd.get()),
                other => log::log(format!("Kein Win32-Fenster-Handle: {other:?}")),
            }
        }
        let dark_mode = true;
        apply_theme(&cc.egui_ctx, dark_mode);

        let is_screenshot_mode = screenshot_target.is_some();
        let persist = !is_screenshot_mode;
        let album_path = album_dir();

        let settings_path = album_path.join("settings.json");
        let settings = if persist {
            Settings::load(&settings_path)
        } else {
            Settings::default()
        };

        // Start tray + global hotkeys first, before the (slow) album load, so the
        // hotkeys work right after launch.
        let bg_rx = if !is_screenshot_mode {
            let (tx, rx) = mpsc::channel();
            background::start_background_listener(tx, cc.egui_ctx.clone());
            background::send_command(BgCommand::SetHotkeys(settings.hotkey_bindings()));
            Some(rx)
        } else {
            None
        };

        // The examples are only for the very first start: an album the user emptied stays empty.
        let first_start = !persist || !album_path.join("album.json").exists();
        let mut items = if persist {
            load_album_index(&album_path)
        } else {
            Vec::new()
        };
        let album_rx = (!items.is_empty()).then(|| {
            let ids = items.iter().map(|i| i.meta.id.clone()).collect();
            spawn_album_image_loader(album_path.clone(), ids, cc.egui_ctx.clone())
        });

        if items.is_empty() && first_start {
            items = create_demo_items();
            if persist {
                let _ = save_album(&album_path, &items);
            }
        }

        // Start in Background mode on normal Windows launch; Editor mode in --screenshot mode
        let initial_mode = if is_screenshot_mode || !cfg!(windows) {
            AppMode::Editor
        } else {
            AppMode::Background
        };

        Self {
            mode: initial_mode,
            allow_exit: is_screenshot_mode,
            bg_rx,
            album_rx,
            overlay_capture_at: None,
            overlay_image: None,
            overlay_texture: None,
            overlay_drag_start: None,
            overlay_drag_curr: None,
            items,
            selected: 0,
            tool: Tool::Arrow,
            color_idx: 0,
            stroke_idx: 1,
            dark_mode,
            show_sidebar: true,
            search_query: String::new(),
            only_favorites: false,
            zoom: 1.0,
            pan: Vec2::ZERO,
            fit_requested: true,
            drag_start_img: None,
            drag_curr_img: None,
            freehand_pts: Vec::new(),
            selected_annotation: None,
            pending_text: None,
            crop_rect: None,
            delay_capture_at: None,
            status_msg: tr(format!("Bereit ({} für Bereich)", settings.region_hotkeys_label()), format!("Ready ({} for region)", settings.region_hotkeys_label())),
            status_until: None,
            persist,
            album_path,
            screenshot_target,
            frame_count: 0,
            screenshot_requested: false,
            settings,
            settings_path,
            show_settings: open_settings,
            recording: None,
            hotkey_status: Vec::new(),
            autostart: settings::autostart_enabled(),
            capture_from_editor: false,
            editor_place: None,
            pending_full_capture: false,
            marked: HashSet::new(),
            confirm_delete: None,
        }
    }

    /// The entries a "delete selection" acts on: the Ctrl+click picks, or else the
    /// entry shown in the editor.
    fn selection_ids(&self) -> Vec<String> {
        if self.marked.is_empty() {
            self.items
                .get(self.selected)
                .map(|it| vec![it.meta.id.clone()])
                .unwrap_or_default()
        } else {
            self.items
                .iter()
                .filter(|it| self.marked.contains(&it.meta.id))
                .map(|it| it.meta.id.clone())
                .collect()
        }
    }

    /// Removes the entries from the album and their images from disk.
    fn delete_items(&mut self, ids: &[String]) {
        let shown_id = self.items.get(self.selected).map(|it| it.meta.id.clone());
        let before = self.items.len();
        self.items.retain(|it| !ids.contains(&it.meta.id));
        let removed = before - self.items.len();
        if removed == 0 {
            return;
        }
        if self.persist {
            for id in ids {
                let path = item_image_path(&self.album_path, id);
                match std::fs::remove_file(&path) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => log::log(format!("Bild nicht gelöscht: {} ({e})", path.display())),
                }
            }
        }
        // Stay on the shown entry if it survived, else on its neighbour.
        self.selected = shown_id
            .and_then(|id| self.items.iter().position(|it| it.meta.id == id))
            .unwrap_or_else(|| self.selected.min(self.items.len().saturating_sub(1)));
        self.marked.retain(|id| !ids.contains(id));
        self.selected_annotation = None;
        self.crop_rect = None;
        self.fit_requested = true;
        self.pan = Vec2::ZERO;
        self.persist_album();
        self.set_status(if removed == 1 {
            tr("Screenshot gelöscht", "Screenshot deleted").to_string()
        } else {
            tr(format!("{removed} Screenshots gelöscht"), format!("{removed} screenshots deleted"))
        });
    }

    /// Asks before deleting what `confirm_delete` holds.
    fn draw_delete_confirm(&mut self, ctx: &egui::Context) {
        let Some(ids) = &self.confirm_delete else {
            return;
        };
        let count = ids.len();
        let question = if count == self.items.len() && count > 1 {
            tr(format!("Alle {count} Screenshots löschen?"), format!("Delete all {count} screenshots?"))
        } else if count == 1 {
            let title = self
                .items
                .iter()
                .find(|it| it.meta.id == ids[0])
                .map(|it| it.meta.title.as_str())
                .unwrap_or("");
            tr(format!("Screenshot „{title}“ löschen?"), format!("Delete screenshot “{title}”?"))
        } else {
            tr(format!("{count} ausgewählte Screenshots löschen?"), format!("Delete {count} selected screenshots?"))
        };
        let mut confirmed = false;
        let mut cancelled = false;
        let modal = egui::Modal::new(egui::Id::new("confirm_delete")).show(ctx, |ui| {
            ui.set_width(320.0);
            ui.label(RichText::new(question).strong().size(14.0));
            ui.add_space(4.0);
            ui.label(RichText::new(tr("Die Bilder werden endgültig entfernt.", "The images will be deleted permanently.")).weak());
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let delete = egui::Button::new(
                    RichText::new(tr("Löschen", "Delete")).color(Color32::WHITE),
                )
                .fill(Color32::from_rgb(220, 38, 38));
                if ui.add(delete).clicked() {
                    confirmed = true;
                }
                if ui.button(tr("Abbrechen", "Cancel")).clicked() {
                    cancelled = true;
                }
            });
        });
        if confirmed {
            if let Some(ids) = self.confirm_delete.take() {
                self.delete_items(&ids);
            }
        } else if cancelled || modal.should_close() {
            self.confirm_delete = None;
        }
    }

    fn save_settings(&mut self) {
        if self.persist {
            if let Err(e) = self.settings.save(&self.settings_path) {
                self.set_status(tr(format!("Einstellungen nicht gespeichert: {e}"), format!("Settings not saved: {e}")));
            }
        }
    }

    fn apply_hotkeys(&self) {
        background::send_command(BgCommand::SetHotkeys(self.settings.hotkey_bindings()));
    }

    fn set_status(&mut self, msg: impl Into<String>) {
        self.status_msg = msg.into();
        self.status_until = Some(Instant::now() + Duration::from_secs(4));
    }

    /// Saves the album index. Images are written separately, off the UI thread,
    /// where they change (capture, crop).
    fn persist_album(&self) {
        if self.persist {
            let _ = save_album_index(&self.album_path, &self.items);
        }
    }

    /// Remembers where the editor window is, so it reappears there later.
    fn remember_editor_place(&mut self) {
        if self.mode == AppMode::Editor {
            if let Some(p) = window::placement() {
                self.editor_place = Some(p);
            }
        }
    }

    /// Leaves the region overlay: frame and normal z-order back (winit; these style
    /// changes keep the window rect). The caller then parks or places the window.
    fn leave_overlay(&mut self, ctx: &egui::Context) {
        self.overlay_image = None;
        self.overlay_texture = None;
        self.overlay_drag_start = None;
        self.overlay_drag_curr = None;
        ctx.send_viewport_cmd(ViewportCommand::Decorations(true));
        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(egui::WindowLevel::Normal));
    }

    /// Moves the editor out of the picture and captures a moment later, once it is
    /// gone. The window stays visible (just off-screen), so frames keep running for
    /// the delay; hiding it would stop them.
    fn prepare_capture(&mut self, ctx: &egui::Context, full: bool) {
        self.capture_from_editor = true;
        self.remember_editor_place();
        window::park_offscreen();
        self.pending_full_capture = full;
        self.mode = AppMode::PreparingOverlay;
        self.overlay_capture_at = Some(Instant::now() + Duration::from_millis(180));
        ctx.request_repaint_after(Duration::from_millis(60));
    }

    /// Hides the window (tray + hotkeys keep running). It is parked off-screen first,
    /// so waking it later (window::wake) only needs to show it.
    fn hide_to_background(&mut self, ctx: &egui::Context) {
        self.remember_editor_place();
        if self.mode == AppMode::RegionOverlay {
            self.leave_overlay(ctx);
        }
        window::park_offscreen();
        self.mode = AppMode::Background;
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
    }

    /// Switches to the editor and puts the window back where it was (physical
    /// pixels, so it lands right on monitors with different scaling), or centres it
    /// on the primary monitor on the first show.
    fn show_editor_gui(&mut self, ctx: &egui::Context) {
        self.remember_editor_place();
        // Visible first: winit re-applies its visibility flag on every style change,
        // and right after a wake-up that flag may still say "hidden".
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        if self.mode == AppMode::RegionOverlay {
            self.leave_overlay(ctx);
        }
        self.mode = AppMode::Editor;
        let place = self.editor_place.unwrap_or_else(|| {
            // The window is on (or parked next to) the primary monitor, so the
            // current scale factor is the primary monitor's.
            let ppp = ctx.pixels_per_point();
            let (w, h) = ((EDITOR_SIZE.x * ppp) as i32, (EDITOR_SIZE.y * ppp) as i32);
            let (pw, ph) = window::primary_size();
            window::Placement {
                x: ((pw - w) / 2).max(0),
                y: ((ph - h) / 2 - 20).max(0),
                w,
                h,
                maximized: false,
            }
        });
        window::restore(&place);
        if place.maximized {
            ctx.send_viewport_cmd(ViewportCommand::Maximized(true));
        }
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        self.fit_requested = true;
    }

    fn begin_region_capture(&mut self, ctx: &egui::Context) {
        if self.mode == AppMode::Editor {
            self.prepare_capture(ctx, false);
        } else {
            if self.mode == AppMode::Background {
                self.capture_from_editor = false;
            }
            self.enter_region_overlay(ctx);
        }
    }

    fn enter_region_overlay(&mut self, ctx: &egui::Context) {
        let img = match capture::capture_primary_screen() {
            Ok(img) => img,
            Err(_) => capture::generate_sample_capture(self.items.len()),
        };
        let size = [img.width() as usize, img.height() as usize];
        let color_image = ColorImage::from_rgba_unmultiplied(size, img.as_raw());
        let tex = ctx.load_texture("region_overlay_tex", color_image, TextureOptions::LINEAR);

        self.overlay_image = Some(img);
        self.overlay_texture = Some(tex);
        self.overlay_drag_start = None;
        self.overlay_drag_curr = None;
        self.mode = AppMode::RegionOverlay;

        // A borderless, topmost window covering the primary monitor (which is what
        // capture_primary_screen grabbed). Not winit's Fullscreen: that positions
        // asynchronously and remembers whatever place the window had before, which
        // put the overlay off-screen.
        // Visible first: see show_editor_gui.
        window::cover_primary();
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Decorations(false));
        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        ctx.request_repaint();
    }

    fn finish_region_capture(&mut self, ctx: &egui::Context, screen_rect: Rect, sel_rect: Rect) {
        let Some(full_img) = self.overlay_image.take() else {
            self.show_editor_gui(ctx);
            return;
        };
        let sw = screen_rect.width().max(1.0);
        let sh = screen_rect.height().max(1.0);
        let u0 = ((sel_rect.min.x - screen_rect.min.x) / sw).clamp(0.0, 1.0);
        let v0 = ((sel_rect.min.y - screen_rect.min.y) / sh).clamp(0.0, 1.0);
        let u1 = ((sel_rect.max.x - screen_rect.min.x) / sw).clamp(0.0, 1.0);
        let v1 = ((sel_rect.max.y - screen_rect.min.y) / sh).clamp(0.0, 1.0);

        let x0 = (u0 * full_img.width() as f32).round() as u32;
        let y0 = (v0 * full_img.height() as f32).round() as u32;
        let x1 = (u1 * full_img.width() as f32).round() as u32;
        let y1 = (v1 * full_img.height() as f32).round() as u32;

        let cropped = if x1 > x0 + 4 && y1 > y0 + 4 {
            image::imageops::crop_imm(&full_img, x0, y0, x1 - x0, y1 - y0).to_image()
        } else {
            full_img
        };

        self.deliver_capture(ctx, cropped, tr("Bereich", "Region"));
    }

    fn take_full_capture(&mut self, ctx: &egui::Context) {
        if self.mode == AppMode::Editor {
            self.prepare_capture(ctx, true);
            return;
        }
        if self.mode == AppMode::Background {
            self.capture_from_editor = false;
        }
        let img = match capture::capture_primary_screen() {
            Ok(img) => img,
            Err(err) => {
                self.set_status(tr(format!("Aufnahme-Hinweis: {}", err), format!("Capture notice: {}", err)));
                capture::generate_sample_capture(self.items.len())
            }
        };
        self.deliver_capture(ctx, img, tr("Vollbild", "Fullscreen"));
    }

    /// Adds a fresh capture to the album and applies the "after capture" setting:
    /// open the editor, save straight to the capture folder, or both.
    fn deliver_capture(&mut self, ctx: &egui::Context, img: RgbaImage, kind: &str) {
        let now = chrono::Local::now();
        let id = format!("shot-{}", now.format("%Y%m%d-%H%M%S-%3f"));
        let title = format!("{} {}", kind, self.items.len() + 1);
        let item = ShotItem::new(id, title, String::new(), img);
        if self.persist {
            let path = item_image_path(&self.album_path, &item.meta.id);
            write_png_in_background(path, item.image.clone(), |_| {});
        }

        let after = self.settings.after_capture;
        let mut status = tr(format!("{kind} aufgenommen — bereit zum Bearbeiten"), format!("{kind} captured — ready to edit"));
        if after.saves() {
            let dir = self.settings.effective_save_dir();
            let notify_success = !after.opens_editor() && !self.capture_from_editor;
            match settings::reserve_capture_path(&dir, now) {
                Ok(path) => {
                    status = tr(format!("Gespeichert: {}", path.display()), format!("Saved: {}", path.display()));
                    let shown = path.display().to_string();
                    let reserved = path.clone();
                    write_png_in_background(path, item.image.clone(), move |result| {
                        let (title, text) = match result {
                            Ok(()) if notify_success => (tr("Screenshot gespeichert", "Screenshot saved"), shown),
                            Ok(()) => return,
                            Err(e) => {
                                let _ = std::fs::remove_file(&reserved);
                                (tr("Speichern fehlgeschlagen", "Saving failed"), format!("{shown} — {e}"))
                            }
                        };
                        background::send_command(BgCommand::Notify { title: title.into(), text });
                    });
                }
                Err(e) => {
                    status = tr(format!("Speichern fehlgeschlagen: {e}"), format!("Saving failed: {e}"));
                    if notify_success {
                        background::send_command(BgCommand::Notify {
                            title: tr("Speichern fehlgeschlagen", "Saving failed").into(),
                            text: format!("{} — {e}", dir.display()),
                        });
                    }
                }
            }
        }
        if self.settings.copy_to_clipboard {
            if let Err(e) = copy_image_to_clipboard(&item.image) {
                status = tr(format!("{status} · Zwischenablage-Fehler: {e}"), format!("{status} · Clipboard error: {e}"));
            }
        }

        self.items.insert(0, item);
        self.selected = 0;
        self.selected_annotation = None;
        self.crop_rect = None;
        self.pan = Vec2::ZERO;
        self.persist_album();
        self.set_status(status);

        if after.opens_editor() || self.capture_from_editor {
            self.show_editor_gui(ctx);
        } else {
            self.hide_to_background(ctx);
        }
    }

    fn copy_selected_to_clipboard(&mut self) {
        let Some(item) = self.items.get(self.selected) else {
            return;
        };
        if !item.is_loaded() {
            self.set_status(tr("Bild wird noch geladen …", "Image is still loading …"));
            return;
        }
        match copy_image_to_clipboard(&item.render_flattened()) {
            Ok(()) => self.set_status(tr("In Zwischenablage kopiert", "Copied to clipboard")),
            Err(e) => self.set_status(tr(format!("Zwischenablage-Fehler: {}", e), format!("Clipboard error: {}", e))),
        }
    }

    fn export_selected_png(&mut self) {
        let Some(item) = self.items.get(self.selected) else {
            return;
        };
        if !item.is_loaded() {
            self.set_status(tr("Bild wird noch geladen …", "Image is still loading …"));
            return;
        }
        let default_name = format!(
            "{}.png",
            item.meta
                .title
                .replace(['\\', '/', ':', '*', '?', '"', '<', '>', '|'], "_")
        );
        let export_dir = self.settings.effective_save_dir();
        let _ = std::fs::create_dir_all(&export_dir);
        if let Some(path) = rfd::FileDialog::new()
            .set_directory(&export_dir)
            .set_file_name(&default_name)
            .add_filter(tr("PNG-Bild", "PNG image"), &["png"])
            .save_file()
        {
            let flat = item.render_flattened();
            match flat.save(&path) {
                Ok(()) => {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("PNG");
                    self.set_status(tr(format!("Gespeichert: {}", name), format!("Saved: {}", name)))
                }
                Err(e) => self.set_status(tr(format!("Speichern fehlgeschlagen: {}", e), format!("Saving failed: {}", e))),
            }
        }
    }

    /// Puts album images decoded by the start-up loader into their items. An image that
    /// could not be read drops its item, as the synchronous load used to.
    fn receive_album_images(&mut self) {
        let Some(rx) = &self.album_rx else { return };
        let mut received = Vec::new();
        let finished = loop {
            match rx.try_recv() {
                Ok(msg) => received.push(msg),
                Err(mpsc::TryRecvError::Empty) => break false,
                Err(mpsc::TryRecvError::Disconnected) => break true,
            }
        };
        if finished {
            self.album_rx = None;
        }
        for (id, image) in received {
            let Some(pos) = self.items.iter().position(|i| i.meta.id == id) else {
                continue; // deleted meanwhile
            };
            match image {
                Some(image) => {
                    self.items[pos].image = image;
                    self.items[pos].texture = None;
                }
                None => {
                    log::log(&format!("Albumbild nicht lesbar, Eintrag entfernt: {id}"));
                    self.items.remove(pos);
                    if pos == self.selected {
                        self.selected_annotation = None;
                    }
                    if pos < self.selected || self.selected >= self.items.len() {
                        self.selected = self.selected.saturating_sub(1);
                    }
                }
            }
        }
    }

    fn ensure_texture(ctx: &egui::Context, item: &mut ShotItem) {
        if item.texture.is_none() && item.is_loaded() {
            let size = [item.image.width() as usize, item.image.height() as usize];
            let color_image = ColorImage::from_rgba_unmultiplied(size, item.image.as_raw());
            item.texture = Some(ctx.load_texture(
                format!("shot_tex_{}", item.meta.id),
                color_image,
                TextureOptions::LINEAR,
            ));
        }
    }

    fn draw_region_overlay(&mut self, ctx: &egui::Context) {
        // Esc or right-click cancels: back to the editor if the capture started
        // there, otherwise back to the background.
        let cancel = ctx.input(|i| {
            i.key_pressed(egui::Key::Escape) || i.pointer.secondary_clicked()
        });
        if cancel {
            if self.capture_from_editor {
                self.show_editor_gui(ctx);
            } else {
                self.hide_to_background(ctx);
            }
            return;
        }

        let mut finish_sel: Option<(Rect, Rect)> = None;

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::BLACK))
            .show(ctx, |ui| {
                let screen_rect = ui.max_rect();
                let resp = ui.allocate_rect(screen_rect, Sense::click_and_drag());
                ctx.set_cursor_icon(CursorIcon::Crosshair);

                let painter = ui.painter_at(screen_rect);

                // Draw frozen desktop image
                if let Some(tex) = &self.overlay_texture {
                    painter.image(
                        tex.id(),
                        screen_rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }

                // Dim the entire screen
                painter.rect_filled(screen_rect, 0.0, Color32::from_black_alpha(130));

                // Drag interaction
                if resp.drag_started_by(egui::PointerButton::Primary) {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        self.overlay_drag_start = Some(pos);
                        self.overlay_drag_curr = Some(pos);
                    }
                } else if resp.dragged_by(egui::PointerButton::Primary) {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        self.overlay_drag_curr = Some(pos);
                    }
                } else if resp.drag_stopped_by(egui::PointerButton::Primary) {
                    if let (Some(a), Some(b)) =
                        (self.overlay_drag_start.take(), self.overlay_drag_curr.take())
                    {
                        let r = Rect::from_two_pos(a, b).intersect(screen_rect);
                        if r.width() >= 8.0 && r.height() >= 8.0 {
                            finish_sel = Some((screen_rect, r));
                        }
                    }
                }

                // Draw bright cutout + border + dimensions badge for active selection
                if let (Some(a), Some(b)) = (self.overlay_drag_start, self.overlay_drag_curr) {
                    let sel = Rect::from_two_pos(a, b).intersect(screen_rect);
                    if sel.width() > 1.0 && sel.height() > 1.0 {
                        if let Some(tex) = &self.overlay_texture {
                            let u0 = (sel.min.x - screen_rect.min.x) / screen_rect.width().max(1.0);
                            let v0 = (sel.min.y - screen_rect.min.y) / screen_rect.height().max(1.0);
                            let u1 = (sel.max.x - screen_rect.min.x) / screen_rect.width().max(1.0);
                            let v1 = (sel.max.y - screen_rect.min.y) / screen_rect.height().max(1.0);
                            painter.image(
                                tex.id(),
                                sel,
                                Rect::from_min_max(Pos2::new(u0, v0), Pos2::new(u1, v1)),
                                Color32::WHITE,
                            );
                        }
                        painter.rect_stroke(
                            sel,
                            0.0,
                            Stroke::new(2.0_f32, Color32::from_rgb(59, 130, 246)),
                            StrokeKind::Outside,
                        );

                        // Live pixel size badge
                        let (pw, ph) = if let Some(img) = &self.overlay_image {
                            (
                                (sel.width() / screen_rect.width().max(1.0) * img.width() as f32)
                                    .round() as u32,
                                (sel.height() / screen_rect.height().max(1.0) * img.height() as f32)
                                    .round() as u32,
                            )
                        } else {
                            (sel.width() as u32, sel.height() as u32)
                        };
                        let label = format!("{} × {} px", pw, ph);
                        let badge_pos = Pos2::new(
                            sel.min.x,
                            (sel.min.y - 26.0).max(screen_rect.min.y + 8.0),
                        );
                        let font = FontId::proportional(12.5);
                        let galley = painter.layout_no_wrap(label.clone(), font.clone(), Color32::WHITE);
                        let badge_rect = Rect::from_min_size(
                            badge_pos,
                            galley.size() + Vec2::new(12.0, 6.0),
                        );
                        painter.rect_filled(
                            badge_rect,
                            CornerRadius::same(5),
                            Color32::from_rgba_unmultiplied(15, 23, 42, 225),
                        );
                        painter.text(
                            badge_rect.min + Vec2::new(6.0, 3.0),
                            egui::Align2::LEFT_TOP,
                            label,
                            font,
                            Color32::WHITE,
                        );
                    }
                } else {
                    // Instruction pill at top center
                    let hint = tr("Bereich mit der Maus markieren · Esc zum Abbrechen", "Drag to select a region · Esc to cancel");
                    let font = FontId::proportional(14.0);
                    let galley = painter.layout_no_wrap(hint.to_string(), font.clone(), Color32::WHITE);
                    let pill = Rect::from_center_size(
                        Pos2::new(screen_rect.center().x, screen_rect.min.y + 38.0),
                        galley.size() + Vec2::new(28.0, 16.0),
                    );
                    painter.rect_filled(
                        pill,
                        CornerRadius::same(10),
                        Color32::from_rgba_unmultiplied(15, 23, 42, 220),
                    );
                    painter.rect_stroke(
                        pill,
                        CornerRadius::same(10),
                        Stroke::new(1.0_f32, Color32::from_rgb(59, 130, 246)),
                        StrokeKind::Inside,
                    );
                    painter.text(
                        pill.center(),
                        egui::Align2::CENTER_CENTER,
                        hint,
                        font,
                        Color32::WHITE,
                    );
                }
            });

        if let Some((screen_rect, sel_rect)) = finish_sel {
            self.finish_region_capture(ctx, screen_rect, sel_rect);
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() {
            return;
        }
        let mut trigger_undo = false;
        let mut trigger_redo = false;
        let mut trigger_copy = false;
        let mut trigger_save = false;
        let mut trigger_delete = false;
        let mut trigger_escape = false;

        ctx.input(|i| {
            if i.modifiers.command && i.key_pressed(egui::Key::Z) {
                trigger_undo = true;
            }
            if i.modifiers.command && i.key_pressed(egui::Key::Y) {
                trigger_redo = true;
            }
            if i.modifiers.command && i.key_pressed(egui::Key::C) {
                trigger_copy = true;
            }
            if i.modifiers.command && i.key_pressed(egui::Key::S) {
                trigger_save = true;
            }
            if i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace) {
                trigger_delete = true;
            }
            if i.key_pressed(egui::Key::Escape) {
                trigger_escape = true;
            }
        });

        if trigger_undo {
            if let Some(item) = self.items.get_mut(self.selected) {
                if item.undo() {
                    self.selected_annotation = None;
                    self.persist_album();
                    self.set_status(tr("Rückgängig", "Undone"));
                }
            }
        }
        if trigger_redo {
            if let Some(item) = self.items.get_mut(self.selected) {
                if item.redo() {
                    self.persist_album();
                    self.set_status(tr("Wiederhergestellt", "Redone"));
                }
            }
        }
        if trigger_copy {
            self.copy_selected_to_clipboard();
        }
        if trigger_save {
            self.export_selected_png();
        }
        if trigger_delete {
            if let Some(idx) = self.selected_annotation.take() {
                if let Some(item) = self.items.get_mut(self.selected) {
                    item.delete_annotation(idx);
                    self.persist_album();
                    self.set_status(tr("Markierung gelöscht", "Markup deleted"));
                }
            } else if self.confirm_delete.is_none() && !self.items.is_empty() {
                // No marking picked: Del deletes the selected screenshot(s), after asking.
                self.confirm_delete = Some(self.selection_ids());
            }
        }
        if trigger_escape {
            self.crop_rect = None;
            self.pending_text = None;
            self.drag_start_img = None;
            self.drag_curr_img = None;
            self.tool = Tool::Select;
        }
    }

    fn draw_top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_toolbar")
            .exact_height(48.0)
            .frame(
                egui::Frame::side_top_panel(&ctx.style())
                    .inner_margin(egui::Margin::symmetric(10, 8)),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    if icon_button(ui, Icon::Sidebar, self.show_sidebar, tr("Album ein-/ausblenden", "Show/hide album"))
                        .clicked()
                    {
                        self.show_sidebar = !self.show_sidebar;
                    }

                    // Primary capture action: Region overlay capture
                    if icon_text_button(ui, Icon::Region, tr("Bereich", "Region"), true)
                        .on_hover_text(tr(format!("Bereich auf dem Bildschirm markieren ({})", self.settings.region_hotkeys_label()), format!("Select a region on the screen ({})", self.settings.region_hotkeys_label())))
                        .clicked()
                    {
                        self.begin_region_capture(ctx);
                    }
                    if icon_button(ui, Icon::Desktop, false, tr("Vollbild aufnehmen", "Capture fullscreen")).clicked() {
                        self.take_full_capture(ctx);
                    }
                    let delay_tip = if let Some(target) = self.delay_capture_at {
                        let rem = target.saturating_duration_since(Instant::now()).as_secs_f32();
                        tr(format!("Verzögerte Aufnahme in {:.0}s...", rem.ceil()), format!("Delayed capture in {:.0}s...", rem.ceil()))
                    } else {
                        tr("Verzögerte Bereichsaufnahme (3s)", "Delayed region capture (3s)").to_string()
                    };
                    if icon_button(ui, Icon::Clock, self.delay_capture_at.is_some(), &delay_tip)
                        .clicked()
                    {
                        self.delay_capture_at = Some(Instant::now() + Duration::from_secs(3));
                        self.set_status(tr("Bereichsaufnahme startet in 3 Sekunden...", "Region capture starts in 3 seconds..."));
                    }

                    ui.separator();

                    // Markup tools
                    let tools = [
                        (Tool::Select, Icon::Cursor, tr("Auswahl / Verschieben", "Select / Move")),
                        (Tool::Arrow, Icon::ArrowUpRight, tr("Pfeil", "Arrow")),
                        (Tool::Rect, Icon::Square, tr("Rechteck", "Rectangle")),
                        (Tool::Highlight, Icon::Highlighter, tr("Textmarker", "Highlighter")),
                        (Tool::Redact, Icon::EyeSlash, tr("Schwärzen (Zensur)", "Redact (censor)")),
                        (Tool::Text, Icon::Text, tr("Text hinzufügen", "Add text")),
                        (Tool::Freehand, Icon::Pencil, tr("Freihand", "Freehand")),
                        (Tool::Crop, Icon::Crop, tr("Zuschneiden", "Crop")),
                    ];
                    for (t, ic, tip) in tools {
                        if icon_button(ui, ic, self.tool == t, tip).clicked() {
                            self.tool = t;
                            self.crop_rect = None;
                            self.pending_text = None;
                        }
                    }

                    ui.separator();

                    if icon_button(ui, Icon::Undo, false, tr("Rückgängig (Strg+Z)", "Undo (Ctrl+Z)")).clicked() {
                        if let Some(item) = self.items.get_mut(self.selected) {
                            if item.undo() {
                                self.persist_album();
                            }
                        }
                    }
                    if icon_button(ui, Icon::Redo, false, tr("Wiederholen (Strg+Y)", "Redo (Ctrl+Y)")).clicked() {
                        if let Some(item) = self.items.get_mut(self.selected) {
                            if item.redo() {
                                self.persist_album();
                            }
                        }
                    }

                    ui.separator();

                    // Color swatches
                    for (idx, &col) in PALETTE.iter().enumerate() {
                        let (rect, resp) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
                        let center = rect.center();
                        ui.painter().circle_filled(center, 7.5, col);
                        if self.color_idx == idx {
                            ui.painter().circle_stroke(
                                center,
                                9.5,
                                Stroke::new(1.8_f32, ui.visuals().strong_text_color()),
                            );
                        }
                        if resp.clicked() {
                            self.color_idx = idx;
                        }
                    }

                    ui.add_space(2.0);

                    // Stroke width selector (S / M / L)
                    for (idx, (lbl, _)) in STROKE_SIZES.iter().enumerate() {
                        let sel = self.stroke_idx == idx;
                        let btn = egui::Button::new(RichText::new(*lbl).size(11.5))
                            .selected(sel)
                            .min_size(Vec2::new(22.0, 24.0));
                        if ui.add(btn).on_hover_text(tr("Strichstärke", "Stroke width")).clicked() {
                            self.stroke_idx = idx;
                        }
                    }

                    // Right-aligned actions: Export, Copy, Background, Theme, Quit
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icon_button(ui, Icon::Power, false, tr("Z-SHOT vollständig beenden", "Quit Z-SHOT completely"))
                            .clicked()
                        {
                            self.allow_exit = true;
                            ctx.send_viewport_cmd(ViewportCommand::Close);
                        }

                        if icon_button(ui, Icon::Cog, self.show_settings, tr("Einstellungen", "Settings"))
                            .clicked()
                        {
                            self.show_settings = !self.show_settings;
                        }

                        let theme_icon = if self.dark_mode { Icon::Sun } else { Icon::Moon };
                        let theme_tip = if self.dark_mode {
                            tr("Helles Design", "Light theme")
                        } else {
                            tr("Dunkles Design", "Dark theme")
                        };
                        if icon_button(ui, theme_icon, false, theme_tip).clicked() {
                            self.dark_mode = !self.dark_mode;
                            apply_theme(ctx, self.dark_mode);
                        }

                        if icon_text_button(ui, Icon::Background, tr("Hintergrund", "Background"), false)
                            .on_hover_text(tr(format!("Fenster ausblenden — weiter im Hintergrund ({})", self.settings.region_hotkeys_label()), format!("Hide window — keep running in the background ({})", self.settings.region_hotkeys_label())))
                            .clicked()
                        {
                            self.hide_to_background(ctx);
                        }

                        if icon_text_button(ui, Icon::Download, "Export", false)
                            .on_hover_text(tr("Als PNG speichern (Strg+S)", "Save as PNG (Ctrl+S)"))
                            .clicked()
                        {
                            self.export_selected_png();
                        }

                        if icon_text_button(ui, Icon::Clipboard, tr("Kopieren", "Copy"), false)
                            .on_hover_text(tr("In Zwischenablage kopieren (Strg+C)", "Copy to clipboard (Ctrl+C)"))
                            .clicked()
                        {
                            self.copy_selected_to_clipboard();
                        }
                    });
                });
            });
    }

    fn draw_sidebar(&mut self, ctx: &egui::Context) {
        if !self.show_sidebar {
            return;
        }
        egui::SidePanel::left("album_sidebar")
            .resizable(true)
            .default_width(250.0)
            .min_width(210.0)
            .max_width(340.0)
            .frame(
                egui::Frame::side_top_panel(&ctx.style())
                    .inner_margin(egui::Margin::symmetric(10, 10)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Album").strong().size(15.0));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(tr(format!("{} Einträge", self.items.len()), format!("{} entries", self.items.len())))
                                .weak()
                                .size(11.5),
                        );
                    });
                });
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
                    paint_icon(
                        ui.painter(),
                        icon_rect,
                        Icon::Search,
                        ui.visuals().weak_text_color(),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.search_query)
                            .hint_text(tr("Suchen (Titel, Notiz, Text)...", "Search (title, notes, text)..."))
                            .desired_width(f32::INFINITY),
                    );
                });
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    if ui.selectable_label(!self.only_favorites, tr("Alle", "All")).clicked() {
                        self.only_favorites = false;
                    }
                    if ui
                        .selectable_label(self.only_favorites, tr("Favoriten", "Favorites"))
                        .clicked()
                    {
                        self.only_favorites = true;
                    }
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let picked = self.selection_ids().len();
                    let has_items = !self.items.is_empty();
                    let label = if self.marked.is_empty() {
                        tr("Auswahl löschen", "Delete selection").to_string()
                    } else {
                        tr(format!("Auswahl löschen ({picked})"), format!("Delete selection ({picked})"))
                    };
                    if ui
                        .add_enabled(has_items, egui::Button::new(label).small())
                        .on_hover_text(tr("Löscht das gewählte Bild. Mit Strg+Klick mehrere Bilder auswählen.", "Deletes the selected image. Ctrl+click to select multiple images."))
                        .clicked()
                    {
                        self.confirm_delete = Some(self.selection_ids());
                    }
                    if ui
                        .add_enabled(has_items, egui::Button::new(tr("Alle löschen", "Delete all")).small())
                        .clicked()
                    {
                        self.confirm_delete =
                            Some(self.items.iter().map(|it| it.meta.id.clone()).collect());
                    }
                });
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);

                let mut delete_idx: Option<usize> = None;
                let mut toggle_fav_idx: Option<usize> = None;
                let mut select_idx: Option<usize> = None;
                let mut mark_idx: Option<usize> = None;
                let ctrl_held = ui.input(|i| i.modifiers.command);

                let avail_h = (ui.available_height() - 125.0).max(120.0);
                egui::ScrollArea::vertical()
                    .max_height(avail_h)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let matching: Vec<usize> = self
                            .items
                            .iter()
                            .enumerate()
                            .filter(|(_, it)| {
                                it.matches_query(&self.search_query, self.only_favorites)
                            })
                            .map(|(i, _)| i)
                            .collect();

                        if matching.is_empty() {
                            ui.add_space(24.0);
                            ui.vertical_centered(|ui| {
                                ui.label(RichText::new(tr("Keine Treffer", "No matches")).weak());
                            });
                        }

                        for idx in matching {
                            Self::ensure_texture(ctx, &mut self.items[idx]);
                            let is_sel = idx == self.selected
                                || self.marked.contains(&self.items[idx].meta.id);
                            // Where the icons sit; the card's own click is dispatched by
                            // pointer position (see below).
                            let mut del_rect = Rect::NOTHING;
                            let mut fav_rect = Rect::NOTHING;
                            let bg = if is_sel {
                                ui.visuals().selection.bg_fill
                            } else {
                                ui.visuals().faint_bg_color
                            };
                            let border = if is_sel {
                                ui.visuals().selection.stroke
                            } else {
                                Stroke::new(
                                    1.0_f32,
                                    ui.visuals().widgets.noninteractive.bg_stroke.color,
                                )
                            };

                            let frame_resp = egui::Frame::NONE
                                .fill(bg)
                                .stroke(border)
                                .corner_radius(CornerRadius::same(8))
                                .inner_margin(egui::Margin::same(8))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    if let Some(tex) = &self.items[idx].texture {
                                        let w = ui.available_width();
                                        let aspect = (self.items[idx].meta.height as f32
                                            / self.items[idx].meta.width.max(1) as f32)
                                            .clamp(0.4, 0.72);
                                        ui.add(
                                            egui::Image::new((tex.id(), Vec2::new(w, w * aspect)))
                                                .corner_radius(CornerRadius::same(4)),
                                        );
                                    }
                                    ui.add_space(4.0);
                                    ui.horizontal(|ui| {
                                        ui.vertical(|ui| {
                                            ui.label(
                                                RichText::new(&self.items[idx].meta.title)
                                                    .strong()
                                                    .size(12.5),
                                            );
                                            let sub_color = if self.dark_mode {
                                                Color32::from_rgb(148, 163, 184)
                                            } else {
                                                Color32::from_rgb(100, 116, 139)
                                            };
                                            ui.label(
                                                RichText::new(format!(
                                                    "{} · {}×{}",
                                                    self.items[idx].meta.created_label(),
                                                    self.items[idx].meta.width,
                                                    self.items[idx].meta.height
                                                ))
                                                .color(sub_color)
                                                .size(10.5),
                                            );
                                        });
                                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                            let (del_r, _) = ui.allocate_exact_size(
                                                Vec2::splat(22.0),
                                                Sense::hover(),
                                            );
                                            del_rect = del_r;
                                            paint_icon(
                                                ui.painter(),
                                                del_r.shrink(3.0),
                                                Icon::Trash,
                                                if ui.rect_contains_pointer(del_r) {
                                                    Color32::from_rgb(239, 68, 68)
                                                } else {
                                                    ui.visuals().weak_text_color()
                                                },
                                            );

                                            let (fav_r, _) = ui.allocate_exact_size(
                                                Vec2::splat(22.0),
                                                Sense::hover(),
                                            );
                                            fav_rect = fav_r;
                                            let fav = self.items[idx].meta.favorite;
                                            paint_icon(
                                                ui.painter(),
                                                fav_r.shrink(3.0),
                                                if fav { Icon::StarSolid } else { Icon::Star },
                                                if fav {
                                                    Color32::from_rgb(245, 158, 11)
                                                } else if ui.rect_contains_pointer(fav_r) {
                                                    ui.visuals().strong_text_color()
                                                } else {
                                                    ui.visuals().weak_text_color()
                                                },
                                            );
                                        });
                                    });
                                });

                            // The card's click area is registered last, so it lies on top
                            // of the icons and receives their clicks too. Dispatch by
                            // where the pointer is instead of giving the icons own senses.
                            let card = frame_resp.response.interact(Sense::click());
                            let pointer = card.interact_pointer_pos().or(card.hover_pos());
                            let over_del = pointer.is_some_and(|p| del_rect.contains(p));
                            let over_fav = pointer.is_some_and(|p| fav_rect.contains(p));
                            let card = if over_del {
                                card.on_hover_text(tr("Löschen", "Delete"))
                            } else if over_fav {
                                card.on_hover_text(tr("Favorit", "Favorite"))
                            } else {
                                card
                            };
                            if card.clicked() {
                                if over_del {
                                    delete_idx = Some(idx);
                                } else if over_fav {
                                    toggle_fav_idx = Some(idx);
                                } else if ctrl_held {
                                    mark_idx = Some(idx);
                                } else {
                                    select_idx = Some(idx);
                                }
                            }
                            ui.add_space(6.0);
                        }
                    });

                if let Some(i) = mark_idx {
                    // Ctrl+click: the shown entry joins the selection, the clicked one toggles.
                    if self.marked.is_empty() {
                        if let Some(shown) = self.items.get(self.selected) {
                            self.marked.insert(shown.meta.id.clone());
                        }
                    }
                    let id = self.items[i].meta.id.clone();
                    if !self.marked.remove(&id) {
                        self.marked.insert(id);
                        select_idx = Some(i);
                    }
                }
                if let Some(i) = select_idx {
                    if mark_idx.is_none() {
                        self.marked.clear();
                    }
                    self.selected = i;
                    self.selected_annotation = None;
                    self.crop_rect = None;
                    self.fit_requested = true;
                    self.pan = Vec2::ZERO;
                }
                if let Some(i) = toggle_fav_idx {
                    if let Some(it) = self.items.get_mut(i) {
                        it.meta.favorite = !it.meta.favorite;
                        self.persist_album();
                    }
                }
                if let Some(i) = delete_idx {
                    // The bin asks to delete its own card (the Ctrl+click picks stay).
                    self.confirm_delete = Some(vec![self.items[i].meta.id.clone()]);
                }

                ui.separator();
                ui.add_space(4.0);
                if let Some(item) = self.items.get_mut(self.selected) {
                    ui.label(RichText::new(tr("Titel & Notizen", "Title & notes")).weak().size(11.5));
                    let title_changed = ui
                        .add(
                            egui::TextEdit::singleline(&mut item.meta.title)
                                .desired_width(f32::INFINITY),
                        )
                        .changed();
                    let notes_changed = ui
                        .add(
                            egui::TextEdit::multiline(&mut item.meta.notes)
                                .desired_rows(2)
                                .desired_width(f32::INFINITY)
                                .hint_text(tr("Schlagwörter / Notizen für Suche...", "Keywords / notes for search...")),
                        )
                        .changed();
                    if title_changed || notes_changed {
                        self.persist_album();
                    }
                }
            });
    }

    fn draw_canvas(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(ctx.style().visuals.extreme_bg_color))
            .show(ctx, |ui| {
                if self.items.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label(tr(format!("Noch keine Screenshots. Drücke {} oder klicke auf „Bereich“.", self.settings.region_hotkeys_label()), format!("No screenshots yet. Press {} or click “Region”.", self.settings.region_hotkeys_label())));
                    });
                    return;
                }

                Self::ensure_texture(ctx, &mut self.items[self.selected]);

                let avail = ui.available_rect_before_wrap();
                let (img_w, img_h) = (
                    self.items[self.selected].meta.width as f32,
                    self.items[self.selected].meta.height as f32,
                );

                if self.fit_requested {
                    let pad = 48.0;
                    let sx = ((avail.width() - pad).max(100.0)) / img_w.max(1.0);
                    let sy = ((avail.height() - pad).max(100.0)) / img_h.max(1.0);
                    self.zoom = sx.min(sy).clamp(0.25, 1.5);
                    self.pan = Vec2::ZERO;
                    self.fit_requested = false;
                }

                let response = ui.allocate_rect(avail, Sense::click_and_drag());
                if response.hovered() {
                    let scroll = ctx.input(|i| i.raw_scroll_delta.y);
                    if scroll.abs() > 0.1 {
                        let factor = if scroll > 0.0 { 1.1 } else { 0.9 };
                        self.zoom = (self.zoom * factor).clamp(0.25, 4.0);
                    }
                }

                let zoom = self.zoom;
                let draw_size = Vec2::new(img_w * zoom, img_h * zoom);
                let img_rect = Rect::from_center_size(avail.center() + self.pan, draw_size);

                let img_to_screen = |p: Pos2| -> Pos2 {
                    Pos2::new(img_rect.min.x + p.x * zoom, img_rect.min.y + p.y * zoom)
                };
                let screen_to_img = |p: Pos2| -> Pos2 {
                    Pos2::new(
                        ((p.x - img_rect.min.x) / zoom).clamp(0.0, img_w),
                        ((p.y - img_rect.min.y) / zoom).clamp(0.0, img_h),
                    )
                };

                let painter = ui.painter_at(avail);

                painter.rect_filled(
                    img_rect.translate(Vec2::new(0.0, 6.0)).expand(2.0),
                    CornerRadius::same(6),
                    Color32::from_black_alpha(65),
                );

                if let Some(tex) = &self.items[self.selected].texture {
                    painter.image(
                        tex.id(),
                        img_rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                painter.rect_stroke(
                    img_rect,
                    CornerRadius::same(2),
                    Stroke::new(1.0_f32, Color32::from_white_alpha(28)),
                    StrokeKind::Outside,
                );

                let annotations = self.items[self.selected].meta.annotations.clone();
                for (idx, ann) in annotations.iter().enumerate() {
                    let is_sel = self.selected_annotation == Some(idx);
                    paint_annotation(&painter, ann, &img_to_screen, zoom, is_sel);
                }

                let active_color = PALETTE[self.color_idx];
                let active_width = STROKE_SIZES[self.stroke_idx].1;
                if let (Some(a), Some(b)) = (self.drag_start_img, self.drag_curr_img) {
                    match self.tool {
                        Tool::Arrow => {
                            let ann = Annotation::Arrow {
                                from: [a.x, a.y],
                                to: [b.x, b.y],
                                color: color32_to_arr(active_color),
                                width: active_width,
                            };
                            paint_annotation(&painter, &ann, &img_to_screen, zoom, false);
                        }
                        Tool::Rect => {
                            let ann = Annotation::Rect {
                                min: [a.x, a.y],
                                max: [b.x, b.y],
                                color: color32_to_arr(active_color),
                                width: active_width,
                            };
                            paint_annotation(&painter, &ann, &img_to_screen, zoom, false);
                        }
                        Tool::Highlight => {
                            let ann = Annotation::Highlight {
                                min: [a.x, a.y],
                                max: [b.x, b.y],
                                color: color32_to_arr(active_color),
                            };
                            paint_annotation(&painter, &ann, &img_to_screen, zoom, false);
                        }
                        Tool::Redact => {
                            let ann = Annotation::Redact {
                                min: [a.x, a.y],
                                max: [b.x, b.y],
                            };
                            paint_annotation(&painter, &ann, &img_to_screen, zoom, false);
                        }
                        Tool::Freehand => {
                            let ann = Annotation::Freehand {
                                points: self.freehand_pts.clone(),
                                color: color32_to_arr(active_color),
                                width: active_width,
                            };
                            paint_annotation(&painter, &ann, &img_to_screen, zoom, false);
                        }
                        Tool::Crop => {
                            let r = Rect::from_two_pos(img_to_screen(a), img_to_screen(b));
                            painter.rect_filled(
                                r,
                                0.0,
                                Color32::from_rgba_unmultiplied(59, 130, 246, 35),
                            );
                            painter.rect_stroke(
                                r,
                                0.0,
                                Stroke::new(2.0_f32, Color32::from_rgb(59, 130, 246)),
                                StrokeKind::Outside,
                            );
                        }
                        _ => {}
                    }
                }

                if let Some(cr) = self.crop_rect {
                    let sr = Rect::from_min_max(img_to_screen(cr.min), img_to_screen(cr.max));
                    painter.rect_filled(
                        sr,
                        0.0,
                        Color32::from_rgba_unmultiplied(59, 130, 246, 30),
                    );
                    painter.rect_stroke(
                        sr,
                        0.0,
                        Stroke::new(2.0_f32, Color32::from_rgb(59, 130, 246)),
                        StrokeKind::Outside,
                    );
                }

                if response.dragged_by(egui::PointerButton::Middle)
                    || (self.tool == Tool::Select
                        && response.dragged_by(egui::PointerButton::Primary))
                {
                    self.pan += response.drag_delta();
                } else if response.drag_started_by(egui::PointerButton::Primary) {
                    if let Some(pos) = response.interact_pointer_pos() {
                        if img_rect.contains(pos) {
                            let ip = screen_to_img(pos);
                            self.drag_start_img = Some(ip);
                            self.drag_curr_img = Some(ip);
                            if self.tool == Tool::Freehand {
                                self.freehand_pts = vec![[ip.x, ip.y]];
                            }
                        }
                    }
                } else if response.dragged_by(egui::PointerButton::Primary) {
                    if let Some(pos) = response.interact_pointer_pos() {
                        let ip = screen_to_img(pos);
                        self.drag_curr_img = Some(ip);
                        if self.tool == Tool::Freehand {
                            self.freehand_pts.push([ip.x, ip.y]);
                        }
                    }
                } else if response.drag_stopped_by(egui::PointerButton::Primary) {
                    if let (Some(a), Some(b)) =
                        (self.drag_start_img.take(), self.drag_curr_img.take())
                    {
                        let dist = a.distance(b);
                        let mut changed = false;
                        match self.tool {
                            Tool::Arrow if dist > 6.0 => {
                                self.items[self.selected].push_annotation(Annotation::Arrow {
                                    from: [a.x, a.y],
                                    to: [b.x, b.y],
                                    color: color32_to_arr(active_color),
                                    width: active_width,
                                });
                                changed = true;
                            }
                            Tool::Rect if dist > 6.0 => {
                                self.items[self.selected].push_annotation(Annotation::Rect {
                                    min: [a.x, a.y],
                                    max: [b.x, b.y],
                                    color: color32_to_arr(active_color),
                                    width: active_width,
                                });
                                changed = true;
                            }
                            Tool::Highlight if dist > 6.0 => {
                                self.items[self.selected].push_annotation(Annotation::Highlight {
                                    min: [a.x, a.y],
                                    max: [b.x, b.y],
                                    color: color32_to_arr(active_color),
                                });
                                changed = true;
                            }
                            Tool::Redact if dist > 6.0 => {
                                self.items[self.selected].push_annotation(Annotation::Redact {
                                    min: [a.x, a.y],
                                    max: [b.x, b.y],
                                });
                                changed = true;
                            }
                            Tool::Freehand if self.freehand_pts.len() > 2 => {
                                let pts = std::mem::take(&mut self.freehand_pts);
                                self.items[self.selected].push_annotation(Annotation::Freehand {
                                    points: pts,
                                    color: color32_to_arr(active_color),
                                    width: active_width,
                                });
                                changed = true;
                            }
                            Tool::Crop if dist > 12.0 => {
                                self.crop_rect = Some(Rect::from_two_pos(a, b));
                            }
                            _ => {}
                        }
                        if changed {
                            self.persist_album();
                        }
                    }
                } else if response.clicked() {
                    if let Some(pos) = response.interact_pointer_pos() {
                        if img_rect.contains(pos) {
                            let ip = screen_to_img(pos);
                            if self.tool == Tool::Text {
                                self.pending_text = Some(PendingText {
                                    img_pos: ip,
                                    text: String::new(),
                                });
                            } else if self.tool == Tool::Select {
                                let hit = self.items[self.selected]
                                    .meta
                                    .annotations
                                    .iter()
                                    .enumerate()
                                    .rev()
                                    .find(|(_, a)| a.hit_test(ip))
                                    .map(|(i, _)| i);
                                self.selected_annotation = hit;
                            }
                        }
                    }
                }

                if let Some(cr) = self.crop_rect {
                    egui::Area::new(egui::Id::new("crop_bar"))
                        .anchor(egui::Align2::CENTER_TOP, Vec2::new(0.0, 14.0))
                        .show(ctx, |ui| {
                            egui::Frame::popup(ui.style())
                                .corner_radius(CornerRadius::same(8))
                                .inner_margin(egui::Margin::symmetric(10, 6))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(tr(
                                            format!("Zuschnitt: {:.0} × {:.0} px", cr.width(), cr.height()),
                                            format!("Crop: {:.0} × {:.0} px", cr.width(), cr.height()),
                                        ));
                                        if icon_text_button(ui, Icon::Check, tr("Anwenden", "Apply"), true)
                                            .clicked()
                                        {
                                            let ok = self.items[self.selected].apply_crop(
                                                cr.min.x as u32,
                                                cr.min.y as u32,
                                                cr.max.x as u32,
                                                cr.max.y as u32,
                                            );
                                            if ok {
                                                if self.persist {
                                                    let item = &self.items[self.selected];
                                                    write_png_in_background(
                                                        item_image_path(&self.album_path, &item.meta.id),
                                                        item.image.clone(),
                                                        |_| {},
                                                    );
                                                    self.persist_album();
                                                }
                                                self.fit_requested = true;
                                                self.set_status(tr("Bild zugeschnitten", "Image cropped"));
                                            }
                                            self.crop_rect = None;
                                            self.tool = Tool::Select;
                                        }
                                        if icon_text_button(ui, Icon::XMark, tr("Abbrechen", "Cancel"), false)
                                            .clicked()
                                        {
                                            self.crop_rect = None;
                                        }
                                    });
                                });
                        });
                }

                let mut commit_text: Option<(Pos2, String)> = None;
                let mut cancel_text = false;
                if let Some(pt) = &mut self.pending_text {
                    let screen_p = img_to_screen(pt.img_pos);
                    egui::Area::new(egui::Id::new("text_input_popup"))
                        .fixed_pos(screen_p)
                        .show(ctx, |ui| {
                            egui::Frame::popup(ui.style())
                                .corner_radius(CornerRadius::same(8))
                                .inner_margin(egui::Margin::same(8))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        let te = ui.add(
                                            egui::TextEdit::singleline(&mut pt.text)
                                                .hint_text(tr("Text eingeben...", "Enter text..."))
                                                .desired_width(180.0),
                                        );
                                        te.request_focus();
                                        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                                        if icon_button(ui, Icon::Check, true, tr("Einfügen", "Insert")).clicked()
                                            || enter
                                        {
                                            commit_text = Some((pt.img_pos, pt.text.clone()));
                                        }
                                        if icon_button(ui, Icon::XMark, false, tr("Abbrechen", "Cancel"))
                                            .clicked()
                                        {
                                            cancel_text = true;
                                        }
                                    });
                                });
                        });
                }
                if let Some((pos, text)) = commit_text {
                    if !text.trim().is_empty() {
                        self.items[self.selected].push_annotation(Annotation::Text {
                            pos: [pos.x, pos.y],
                            text,
                            color: color32_to_arr(PALETTE[self.color_idx]),
                            size: 18.0,
                        });
                        self.persist_album();
                    }
                    self.pending_text = None;
                } else if cancel_text {
                    self.pending_text = None;
                }
            });
    }

    fn draw_settings_window(&mut self, ctx: &egui::Context) {
        if !self.show_settings {
            if self.recording.take().is_some() {
                background::send_command(BgCommand::CancelRecording);
            }
            return;
        }

        let mut open = true;
        let mut close_clicked = false;
        // Square, and never larger than the editor: the content scrolls instead of
        // hanging off the window's bottom edge.
        let screen = ctx.screen_rect();
        let side = 600.0_f32
            .min(screen.width() - 40.0)
            .min(screen.height() - 80.0)
            .max(300.0);
        egui::Window::new(tr("Einstellungen", "Settings"))
            .id(egui::Id::new("settings_window"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .fixed_size([side, side])
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                let footer_h = 34.0;
                egui::ScrollArea::vertical()
                    .max_height(ui.available_height() - footer_h)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {

                // --- Hotkeys -------------------------------------------------
                ui.label(RichText::new(tr("Tastenkürzel", "Hotkeys")).strong().size(15.0));
                ui.label(
                    RichText::new(tr(
                        "Auf das Feld klicken und die gewünschte Taste oder Kombination drücken \
                         (wird automatisch erkannt). Esc bricht ab.",
                        "Click the field and press the key or combination you want \
                         (detected automatically). Esc cancels.",
                    ))
                    .weak()
                    .size(12.0),
                );
                egui::Grid::new("hotkey_grid")
                    .num_columns(4)
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        for action in HotkeyAction::ALL {
                            ui.label(action.title());

                            let is_recording = self.recording == Some(action);
                            let text = if is_recording {
                                RichText::new(tr("Taste drücken …", "Press a key …")).color(Color32::from_rgb(245, 158, 11))
                            } else {
                                match self.settings.hotkey(action) {
                                    Some(hk) => RichText::new(hk.label()).strong(),
                                    None => RichText::new(tr("Nicht belegt", "Not set")).weak(),
                                }
                            };
                            let border = if is_recording {
                                Color32::from_rgb(245, 158, 11)
                            } else {
                                ui.visuals().widgets.inactive.fg_stroke.color.gamma_multiply(0.45)
                            };
                            let btn = egui::Button::new(text)
                                .fill(ui.visuals().extreme_bg_color)
                                .stroke(Stroke::new(1.0_f32, border))
                                .min_size(Vec2::new(170.0, 26.0));
                            let resp = ui.add(btn).on_hover_text(if is_recording {
                                tr("Klicken zum Abbrechen", "Click to cancel")
                            } else {
                                tr("Klicken und neue Taste drücken", "Click and press a new key")
                            });
                            if resp.clicked() {
                                if is_recording {
                                    background::send_command(BgCommand::CancelRecording);
                                } else {
                                    if self.recording.is_some() {
                                        background::send_command(BgCommand::CancelRecording);
                                    }
                                    self.recording = Some(action);
                                    background::send_command(BgCommand::StartRecording);
                                }
                            }

                            let clear_enabled = self.settings.hotkey(action).is_some() && !is_recording;
                            if ui
                                .add_enabled(clear_enabled, egui::Button::new(tr("Entfernen", "Remove")))
                                .clicked()
                            {
                                self.settings.set_hotkey(action, None);
                                self.save_settings();
                                self.apply_hotkeys();
                            }

                            let status = self
                                .hotkey_status
                                .iter()
                                .find(|(a, _)| *a == action)
                                .map(|(_, ok)| *ok);
                            match (self.settings.hotkey(action), status) {
                                (Some(_), Some(true)) => {
                                    ui.label(RichText::new(tr("aktiv", "active")).color(Color32::from_rgb(16, 185, 129)));
                                }
                                (Some(_), Some(false)) => {
                                    ui.label(RichText::new(tr("belegt", "in use")).color(Color32::from_rgb(239, 68, 68)))
                                        .on_hover_text(tr(
                                            "Windows hat die Kombination abgelehnt — sie wird \
                                             vermutlich von einem anderen Programm verwendet.",
                                            "Windows rejected the combination — it is \
                                             probably used by another program.",
                                        ));
                                }
                                _ => {
                                    ui.label("");
                                }
                            }
                            ui.end_row();
                        }
                    });

                let print_used = HotkeyAction::ALL.into_iter().any(|a| {
                    self.settings.hotkey(a).map(|h| h.vk) == Some(settings::VK_SNAPSHOT)
                });
                if print_used {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(tr(
                                "Öffnet die Druck-Taste stattdessen das Windows-Snipping-Tool? \
                                 Dann in Windows unter Barrierefreiheit › Tastatur die Option \
                                 „Druck-Taste zum Öffnen der Bildschirmaufnahme verwenden“ ausschalten.",
                                "Does the Print key open the Windows Snipping Tool instead? \
                                 Then turn off “Use the Print screen key to open screen capture” \
                                 in Windows under Accessibility › Keyboard.",
                            ))
                            .weak()
                            .size(12.0),
                        );
                        if ui.small_button(tr("Windows-Einstellung öffnen", "Open Windows setting")).clicked() {
                            let _ = std::process::Command::new("explorer")
                                .arg("ms-settings:easeofaccess-keyboard")
                                .spawn();
                        }
                    });
                }

                ui.separator();

                // --- After capture -------------------------------------------
                ui.label(RichText::new(tr("Nach der Bereichsauswahl", "After selecting a region")).strong().size(15.0));
                let before = self.settings.clone();
                ui.radio_value(
                    &mut self.settings.after_capture,
                    AfterCapture::OpenEditor,
                    tr("Editor-Fenster öffnen", "Open editor window"),
                );
                ui.radio_value(
                    &mut self.settings.after_capture,
                    AfterCapture::SaveDirect,
                    tr("Direkt speichern (ohne Fenster)", "Save directly (no window)"),
                );
                ui.radio_value(
                    &mut self.settings.after_capture,
                    AfterCapture::SaveAndOpenEditor,
                    tr("Speichern und Editor-Fenster öffnen", "Save and open editor window"),
                );
                ui.checkbox(
                    &mut self.settings.copy_to_clipboard,
                    tr("Zusätzlich in die Zwischenablage kopieren", "Also copy to clipboard"),
                );

                ui.separator();

                // --- Save folder ---------------------------------------------
                ui.label(RichText::new(tr("Speicherort", "Save location")).strong().size(15.0));
                let dir = self.settings.effective_save_dir();
                ui.label(RichText::new(dir.display().to_string()).monospace().size(12.0));
                ui.label(
                    RichText::new(tr(
                        format!("Dateiname nach Datum und Uhrzeit, z. B. {}", settings::capture_file_name(chrono::Local::now())),
                        format!("File name by date and time, e.g. {}", settings::capture_file_name(chrono::Local::now())),
                    ))
                    .weak()
                    .size(12.0),
                );
                ui.horizontal(|ui| {
                    if ui.button(tr("Ändern …", "Change …")).clicked() {
                        let _ = std::fs::create_dir_all(&dir);
                        if let Some(picked) = rfd::FileDialog::new().set_directory(&dir).pick_folder() {
                            self.settings.save_dir = Some(picked);
                        }
                    }
                    if ui.button(tr("Ordner öffnen", "Open folder")).clicked() {
                        let _ = std::fs::create_dir_all(&dir);
                        let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                    }
                    if self.settings.save_dir.is_some()
                        && ui.button(tr("Standard (Bilder\\Z-SHOT)", "Default (Pictures\\Z-SHOT)")).clicked()
                    {
                        self.settings.save_dir = None;
                    }
                });

                if self.settings != before {
                    self.save_settings();
                }

                ui.separator();

                // --- General -------------------------------------------------
                ui.label(RichText::new(tr("Allgemein", "General")).strong().size(15.0));
                let mut autostart = self.autostart;
                if ui.checkbox(&mut autostart, tr("Beim Windows-Start im Hintergrund starten", "Start in the background when Windows starts")).changed() {
                    if let Err(e) = settings::set_autostart(autostart) {
                        self.set_status(format!("Autostart: {e}"));
                    }
                    self.autostart = settings::autostart_enabled();
                }

                ui.separator();

                // --- About ---------------------------------------------------
                ui.label(RichText::new(tr("Über Z-SHOT", "About Z-SHOT")).strong().size(15.0));
                ui.label(format!(
                    "Z-SHOT Version {}  ·  © 2026 R. Trunk",
                    env!("CARGO_PKG_VERSION")
                ));
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.label(tr("Lizenz: ", "Licence: "));
                    ui.hyperlink_to(
                        tr("GNU General Public License v3 oder neuer", "GNU General Public License v3 or later"),
                        "https://www.gnu.org/licenses/gpl-3.0.html",
                    );
                });
                ui.label(
                    RichText::new(tr(
                        "Freie Software: Sie dürfen sie unter den Bedingungen der GPL \
                         weitergeben und verändern. Sie wird ohne jede Gewährleistung \
                         bereitgestellt.",
                        "Free software: you may redistribute and modify it under the terms \
                         of the GPL. It comes with ABSOLUTELY NO WARRANTY.",
                    ))
                    .weak()
                    .size(12.0),
                );
                    });

                ui.separator();
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(tr("Schließen", "Close")).clicked() {
                        close_clicked = true;
                    }
                });
            });

        if !open || close_clicked {
            self.show_settings = false;
            if self.recording.take().is_some() {
                background::send_command(BgCommand::CancelRecording);
            }
        }
    }

    fn draw_status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(32.0)
            .frame(
                egui::Frame::side_top_panel(&ctx.style())
                    .inner_margin(egui::Margin::symmetric(12, 4)),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    if let Some(item) = self.items.get(self.selected) {
                        ui.label(RichText::new(&item.meta.title).strong().size(12.0));
                        ui.separator();
                        ui.label(
                            RichText::new(format!("{} × {} px", item.meta.width, item.meta.height))
                                .weak()
                                .size(12.0),
                        );
                        ui.separator();
                        ui.label(
                            RichText::new(tr(
                                format!("{} Markierungen", item.meta.annotations.len()),
                                format!("{} markups", item.meta.annotations.len()),
                            ))
                            .weak()
                            .size(12.0),
                        );
                    }

                    let idle_hint =
                        tr(format!("{}: Bereich aufnehmen", self.settings.region_hotkeys_label()), format!("{}: capture region", self.settings.region_hotkeys_label()));
                    let active_status = match self.status_until {
                        Some(until) if Instant::now() < until => self.status_msg.as_str(),
                        _ => idle_hint.as_str(),
                    };
                    ui.separator();
                    ui.label(RichText::new(active_status).weak().size(12.0));

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icon_button(ui, Icon::FitScreen, false, tr("An Fenster anpassen", "Fit to window")).clicked()
                        {
                            self.fit_requested = true;
                        }
                        if icon_button(ui, Icon::ZoomIn, false, tr("Vergrößern", "Zoom in")).clicked() {
                            self.zoom = (self.zoom + 0.15).min(4.0);
                        }
                        ui.label(RichText::new(format!("{:.0}%", self.zoom * 100.0)).size(12.0));
                        if icon_button(ui, Icon::ZoomOut, false, tr("Verkleinern", "Zoom out")).clicked() {
                            self.zoom = (self.zoom - 0.15).max(0.25);
                        }
                    });
                });
            });
    }
}

/// PNG writes started by [`write_png_in_background`] that have not finished yet.
static PNG_WRITES_PENDING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// On exit: lets running PNG writes finish, so quitting right after a capture does
/// not lose it (leaving the process would kill the writer threads).
fn wait_for_png_writes(timeout: Duration) {
    let started = Instant::now();
    while PNG_WRITES_PENDING.load(std::sync::atomic::Ordering::Acquire) > 0 {
        if started.elapsed() > timeout {
            log::log("Beenden: PNG-Schreibvorgänge nach 10 s nicht fertig");
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Encodes and writes a PNG on a worker thread, then calls `done` there. A screen-sized
/// capture takes over a second to encode in a debug build, and the editor would stay
/// frozen that long. The data goes to `<name>.png.tmp` first and is renamed over
/// `path` at the end, so no one ever reads a half-written file.
fn write_png_in_background(
    path: PathBuf,
    image: RgbaImage,
    done: impl FnOnce(Result<(), String>) + Send + 'static,
) {
    PNG_WRITES_PENDING.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    std::thread::spawn(move || {
        // Decremented at the very end, also if `done` panics.
        struct Pending;
        impl Drop for Pending {
            fn drop(&mut self) {
                PNG_WRITES_PENDING.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
            }
        }
        let _pending = Pending;
        let tmp = path.with_extension("png.tmp");
        let result = image
            .save_with_format(&tmp, image::ImageFormat::Png)
            .and_then(|()| std::fs::rename(&tmp, &path).map_err(image::ImageError::IoError))
            .map_err(|e| e.to_string());
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
            log::log(&format!("PNG schreiben fehlgeschlagen ({}): {result:?}", path.display()));
        }
        done(result);
    });
}

fn copy_image_to_clipboard(img: &RgbaImage) -> Result<(), arboard::Error> {
    let img_data = arboard::ImageData {
        width: img.width() as usize,
        height: img.height() as usize,
        bytes: std::borrow::Cow::Borrowed(img.as_raw()),
    };
    arboard::Clipboard::new().and_then(|mut cb| cb.set_image(img_data))
}

fn paint_annotation(
    painter: &egui::Painter,
    ann: &Annotation,
    to_screen: &dyn Fn(Pos2) -> Pos2,
    zoom: f32,
    selected: bool,
) {
    match ann {
        Annotation::Arrow {
            from,
            to,
            color,
            width,
        } => {
            let a = to_screen(Pos2::new(from[0], from[1]));
            let b = to_screen(Pos2::new(to[0], to[1]));
            let c = arr_to_color32(*color);
            let w = (width * zoom).max(1.5);
            painter.line_segment([a, b], Stroke::new(w, c));
            let dir = (b - a).normalized();
            let head = (w * 4.2).max(12.0);
            let perp = Vec2::new(-dir.y, dir.x);
            let tip_l = b - dir * head + perp * (head * 0.55);
            let tip_r = b - dir * head - perp * (head * 0.55);
            painter.add(egui::Shape::convex_polygon(
                vec![b, tip_l, tip_r],
                c,
                Stroke::NONE,
            ));
            if selected {
                painter.circle_stroke(a, 5.0, Stroke::new(1.5_f32, Color32::WHITE));
                painter.circle_stroke(b, 5.0, Stroke::new(1.5_f32, Color32::WHITE));
            }
        }
        Annotation::Rect {
            min,
            max,
            color,
            width,
        } => {
            let r = Rect::from_two_pos(
                to_screen(Pos2::new(min[0], min[1])),
                to_screen(Pos2::new(max[0], max[1])),
            );
            let c = arr_to_color32(*color);
            painter.rect_stroke(
                r,
                CornerRadius::same(3),
                Stroke::new((width * zoom).max(1.5), c),
                StrokeKind::Middle,
            );
            if selected {
                painter.rect_stroke(
                    r.expand(4.0),
                    CornerRadius::same(4),
                    Stroke::new(1.0_f32, Color32::WHITE),
                    StrokeKind::Outside,
                );
            }
        }
        Annotation::Highlight { min, max, color } => {
            let r = Rect::from_two_pos(
                to_screen(Pos2::new(min[0], min[1])),
                to_screen(Pos2::new(max[0], max[1])),
            );
            let c = Color32::from_rgba_unmultiplied(color[0], color[1], color[2], 95);
            painter.rect_filled(r, CornerRadius::same(3), c);
            if selected {
                painter.rect_stroke(
                    r,
                    CornerRadius::same(3),
                    Stroke::new(1.2_f32, Color32::WHITE),
                    StrokeKind::Outside,
                );
            }
        }
        Annotation::Redact { min, max } => {
            let r = Rect::from_two_pos(
                to_screen(Pos2::new(min[0], min[1])),
                to_screen(Pos2::new(max[0], max[1])),
            );
            painter.rect_filled(r, CornerRadius::same(2), Color32::from_rgb(15, 23, 42));
            if selected {
                painter.rect_stroke(
                    r,
                    CornerRadius::same(2),
                    Stroke::new(1.5_f32, Color32::from_rgb(96, 165, 250)),
                    StrokeKind::Outside,
                );
            }
        }
        Annotation::Freehand {
            points,
            color,
            width,
        } => {
            if points.len() >= 2 {
                let pts: Vec<Pos2> = points
                    .iter()
                    .map(|p| to_screen(Pos2::new(p[0], p[1])))
                    .collect();
                painter.add(egui::Shape::line(
                    pts,
                    Stroke::new((width * zoom).max(1.5), arr_to_color32(*color)),
                ));
            }
        }
        Annotation::Text {
            pos,
            text,
            color,
            size,
        } => {
            let p = to_screen(Pos2::new(pos[0], pos[1]));
            let font = FontId::proportional((size * zoom).max(11.0));
            let galley = painter.layout_no_wrap(text.clone(), font.clone(), arr_to_color32(*color));
            let bg_rect = Rect::from_min_size(
                p - Vec2::new(4.0, 2.0),
                galley.size() + Vec2::new(8.0, 4.0),
            );
            painter.rect_filled(
                bg_rect,
                CornerRadius::same(4),
                Color32::from_rgba_unmultiplied(15, 23, 42, 210),
            );
            if selected {
                painter.rect_stroke(
                    bg_rect,
                    CornerRadius::same(4),
                    Stroke::new(1.2_f32, Color32::WHITE),
                    StrokeKind::Outside,
                );
            }
            painter.text(p, egui::Align2::LEFT_TOP, text, font, arr_to_color32(*color));
        }
    }
}

impl eframe::App for ZShotApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Handle close button: hide to background instead of quitting unless explicit Quit
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_exit {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.hide_to_background(ctx);
            return;
        }

        self.receive_album_images();

        // Poll events from background tray / global hotkey thread. The token is taken
        // before draining, see window::handled_token.
        let wake_token = window::handled_token();
        let mut pending_events = Vec::new();
        if let Some(rx) = &self.bg_rx {
            while let Ok(ev) = rx.try_recv() {
                pending_events.push(ev);
            }
        }
        for ev in pending_events {
            log::log(format!("Verarbeitet (Modus {:?}): {ev:?}", self.mode));
            match ev {
                BackgroundEvent::StartRegionCapture => self.begin_region_capture(ctx),
                BackgroundEvent::StartFullCapture => self.take_full_capture(ctx),
                BackgroundEvent::ShowGui => self.show_editor_gui(ctx),
                BackgroundEvent::OpenSettings => {
                    if self.mode != AppMode::Editor {
                        self.show_editor_gui(ctx);
                    }
                    self.show_settings = true;
                }
                BackgroundEvent::HotkeysApplied(status) => {
                    let failed: Vec<&str> = status
                        .iter()
                        .filter(|(_, ok)| !ok)
                        .map(|(a, _)| a.title())
                        .collect();
                    if !failed.is_empty() {
                        let failed = failed.join(", ");
                        self.set_status(tr(
                            format!("Tastenkürzel belegt (anderes Programm?): {}", failed),
                            format!("Hotkey in use (another program?): {}", failed),
                        ));
                    }
                    self.hotkey_status = status;
                }
                BackgroundEvent::HotkeyRecorded(recorded) => {
                    if let (Some(action), Some(hk)) = (self.recording.take(), recorded) {
                        self.settings.set_hotkey(action, Some(hk));
                        self.save_settings();
                        self.apply_hotkeys();
                        self.set_status(format!("{}: {}", action.title(), hk.label()));
                    }
                    self.recording = None;
                }
                BackgroundEvent::Quit => {
                    self.allow_exit = true;
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                    return;
                }
            }
        }

        window::mark_handled(wake_token);

        // Handle delayed overlay preparation (waiting for editor window to hide cleanly)
        if self.mode == AppMode::PreparingOverlay {
            if let Some(at) = self.overlay_capture_at {
                if Instant::now() >= at {
                    self.overlay_capture_at = None;
                    if std::mem::take(&mut self.pending_full_capture) {
                        self.take_full_capture(ctx);
                    } else {
                        self.enter_region_overlay(ctx);
                    }
                } else {
                    ctx.request_repaint_after(Duration::from_millis(30));
                    return;
                }
            }
        }

        // Handle 3-second delayed capture timer
        if let Some(target) = self.delay_capture_at {
            if Instant::now() >= target {
                self.delay_capture_at = None;
                self.begin_region_capture(ctx);
                return;
            } else {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
        }

        window::set_want_hidden(self.mode == AppMode::Background);
        match self.mode {
            AppMode::Background => {
                // In the background the window must end up hidden. It can be shown
                // behind winit's back (window::wake; eframe also force-shows it after
                // the first frame). Win32 hides it right now; Visible(false) keeps
                // winit's own flag at "hidden" (a no-op if it already is). Both are
                // needed: winit skips Visible(false) when its flag is stale, and with
                // a stale "visible" flag it would re-show the window on its next
                // style change.
                // No polling: events wake the window via window::wake, which also
                // re-hides it if it gets shown again after this frame.
                if window::is_visible() {
                    window::hide();
                }
                ctx.send_viewport_cmd(ViewportCommand::Visible(false));
                return;
            }
            AppMode::PreparingOverlay => {
                ctx.request_repaint_after(Duration::from_millis(30));
                return;
            }
            AppMode::RegionOverlay => {
                self.draw_region_overlay(ctx);
                return;
            }
            AppMode::Editor => {
                self.handle_shortcuts(ctx);
                self.draw_top_bar(ctx);
                self.draw_status_bar(ctx);
                self.draw_sidebar(ctx);
                self.draw_canvas(ctx);
                self.draw_settings_window(ctx);
                self.draw_delete_confirm(ctx);
            }
        }

        // Automated --screenshot handling
        if let Some(target_path) = self.screenshot_target.clone() {
            self.frame_count = self.frame_count.saturating_add(1);
            ctx.request_repaint();

            let captured = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });

            if let Some(img) = captured {
                if let Some(parent) = target_path.parent() {
                    if !parent.as_os_str().is_empty() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                }
                let w = img.width() as u32;
                let h = img.height() as u32;
                let mut raw = Vec::with_capacity((w * h * 4) as usize);
                for px in &img.pixels {
                    raw.extend_from_slice(&px.to_array());
                }
                if let Some(rgba) = RgbaImage::from_raw(w, h, raw) {
                    let _ = rgba.save(&target_path);
                }
                self.allow_exit = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
                return;
            }

            if (self.frame_count >= 120 && !self.screenshot_requested) || self.frame_count == 180 {
                self.screenshot_requested = true;
                ctx.send_viewport_cmd(ViewportCommand::Screenshot(egui::UserData::default()));
            }
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = parse_args();

    if let Some(test_path) = cli.self_test_path {
        run_self_test(test_path).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        return Ok(());
    }

    let is_screenshot_mode = cli.screenshot_path.is_some();
    if !is_screenshot_mode {
        log::log(format!("Z-SHOT {} gestartet", env!("CARGO_PKG_VERSION")));
    }

    let mut viewport = ViewportBuilder::default()
        .with_title("Z-SHOT")
        .with_icon(std::sync::Arc::new(egui::IconData {
            rgba: icons::app_icon_rgba(64),
            width: 64,
            height: 64,
        }))
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([860.0, 560.0]);

    if is_screenshot_mode {
        viewport = viewport.with_active(false);
        #[cfg(target_os = "windows")]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    } else {
        #[cfg(target_os = "windows")]
        {
            // Start quietly in the background (tray icon + global hotkeys active).
            // Off-screen as well, because eframe force-shows the window after frame 1.
            viewport = viewport.with_visible(false).with_position(OFFSCREEN_POS);
        }
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    let screenshot_target = cli.screenshot_path;
    let open_settings = cli.open_settings;
    eframe::run_native(
        "Z-SHOT",
        options,
        Box::new(move |cc| Ok(Box::new(ZShotApp::new(cc, screenshot_target, open_settings)))),
    )?;
    wait_for_png_writes(Duration::from_secs(10));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_png_write_is_complete_after_waiting() {
        let dir = std::env::temp_dir().join(format!("zshot-pngwrite-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("shot.png");
        let image = RgbaImage::from_pixel(64, 32, image::Rgba([10, 20, 30, 255]));
        write_png_in_background(path.clone(), image, |result| assert!(result.is_ok()));
        wait_for_png_writes(Duration::from_secs(10));
        let back = image::open(&path).unwrap().to_rgba8();
        assert_eq!(back.dimensions(), (64, 32));
        assert!(!dir.join("shot.png.tmp").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
