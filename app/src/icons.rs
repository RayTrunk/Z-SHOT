// Z-SHOT — Copyright (C) 2026 R. Trunk
// SPDX-License-Identifier: GPL-3.0-or-later

//! Heroicons (https://heroicons.com, v2.2.0, MIT — see assets/heroicons/LICENSE).
//!
//! The 24×24 outline SVGs are embedded with their colour set to white, rasterised by
//! egui_extras' SVG loader at the exact on-screen pixel size and tinted at paint time.

use eframe::egui::{self, load::SizeHint, load::TexturePoll, Color32, Painter, Pos2, Rect, Stroke, TextureOptions, Vec2};

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Camera,
    Desktop,
    Region,
    Crop,
    Clock,
    Cursor,
    ArrowUpRight,
    Square,
    Highlighter,
    EyeSlash,
    Text,
    Pencil,
    Undo,
    Redo,
    Clipboard,
    Download,
    Trash,
    Star,
    StarSolid,
    Search,
    Sun,
    Moon,
    ZoomIn,
    ZoomOut,
    FitScreen,
    Sidebar,
    Check,
    XMark,
    Background,
    Power,
    Cog,
}

macro_rules! svg {
    ($name:literal) => {
        (
            concat!("bytes://heroicons/", $name, ".svg"),
            include_bytes!(concat!("../assets/heroicons/", $name, ".svg")) as &'static [u8],
        )
    };
}

impl Icon {
    pub const ALL: [Icon; 31] = [
        Icon::Camera, Icon::Desktop, Icon::Region, Icon::Crop, Icon::Clock, Icon::Cursor,
        Icon::ArrowUpRight, Icon::Square, Icon::Highlighter, Icon::EyeSlash, Icon::Text,
        Icon::Pencil, Icon::Undo, Icon::Redo, Icon::Clipboard, Icon::Download, Icon::Trash,
        Icon::Star, Icon::StarSolid, Icon::Search, Icon::Sun, Icon::Moon, Icon::ZoomIn,
        Icon::ZoomOut, Icon::FitScreen, Icon::Sidebar, Icon::Check, Icon::XMark,
        Icon::Background, Icon::Power, Icon::Cog,
    ];

    /// Loader URI and embedded SVG bytes of the Heroicon used for this icon.
    pub fn source(self) -> (&'static str, &'static [u8]) {
        match self {
            Icon::Camera => svg!("camera"),
            Icon::Desktop => svg!("computer-desktop"),
            Icon::Region => svg!("viewfinder-circle"),
            Icon::Crop => svg!("scissors"),
            Icon::Clock => svg!("clock"),
            Icon::Cursor => svg!("cursor-arrow-rays"),
            Icon::ArrowUpRight => svg!("arrow-up-right"),
            Icon::Square => svg!("stop"),
            Icon::Highlighter => svg!("paint-brush"),
            Icon::EyeSlash => svg!("eye-slash"),
            Icon::Text => svg!("chat-bubble-bottom-center-text"),
            Icon::Pencil => svg!("pencil"),
            Icon::Undo => svg!("arrow-uturn-left"),
            Icon::Redo => svg!("arrow-uturn-right"),
            Icon::Clipboard => svg!("clipboard-document"),
            Icon::Download => svg!("arrow-down-tray"),
            Icon::Trash => svg!("trash"),
            Icon::Star => svg!("star"),
            Icon::StarSolid => svg!("star-solid"),
            Icon::Search => svg!("magnifying-glass"),
            Icon::Sun => svg!("sun"),
            Icon::Moon => svg!("moon"),
            Icon::ZoomIn => svg!("magnifying-glass-plus"),
            Icon::ZoomOut => svg!("magnifying-glass-minus"),
            Icon::FitScreen => svg!("arrows-pointing-out"),
            Icon::Sidebar => svg!("bars-3"),
            Icon::Check => svg!("check"),
            Icon::XMark => svg!("x-mark"),
            Icon::Background => svg!("minus"),
            Icon::Power => svg!("power"),
            Icon::Cog => svg!("cog-6-tooth"),
        }
    }
}

/// Installs the SVG loader and registers all embedded icons. Call once at startup.
pub fn install(ctx: &egui::Context) {
    egui_extras::install_image_loaders(ctx);
    for icon in Icon::ALL {
        let (uri, bytes) = icon.source();
        ctx.include_bytes(uri, bytes);
    }
}

/// Paints `icon` into `rect`, tinted with `color`.
pub fn paint_icon(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let ctx = painter.ctx();
    let side = rect.width().max(rect.height());
    let px = (side * ctx.pixels_per_point()).round().max(8.0) as u32;
    let (uri, _) = icon.source();
    match ctx.try_load_texture(uri, TextureOptions::LINEAR, SizeHint::Size(px, px)) {
        Ok(TexturePoll::Ready { texture }) => {
            let square = Rect::from_center_size(rect.center(), Vec2::splat(side));
            painter.image(
                texture.id,
                square,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                color,
            );
        }
        Ok(TexturePoll::Pending { .. }) => ctx.request_repaint(),
        Err(_) => {}
    }
}

/// The Z-SHOT app icon: Heroicons viewfinder corners + amber bolt ("Z") on dark slate.
/// Same file `build.rs` embeds as the executable icon.
pub const APP_ICON_SVG: &str = include_str!("../assets/z-shot-icon.svg");

/// App icon as straight RGBA, for the window/taskbar icon and the tray icon.
pub fn app_icon_rgba(size: u32) -> Vec<u8> {
    use resvg::{tiny_skia, usvg, usvg::TreeParsing};

    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let Ok(tree) = usvg::Tree::from_str(APP_ICON_SVG, &usvg::Options::default()) else {
        return rgba;
    };
    let Some(mut pixmap) = tiny_skia::Pixmap::new(size, size) else {
        return rgba;
    };
    let scale = size as f32 / 24.0;
    resvg::Tree::from_usvg(&tree).render(tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    for (dst, px) in rgba.chunks_exact_mut(4).zip(pixmap.pixels()) {
        let c = px.demultiply();
        dst.copy_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    rgba
}

/// Renders an icon-only toolbar button with active state and tooltip.
pub fn icon_button(
    ui: &mut egui::Ui,
    icon: Icon,
    selected: bool,
    tooltip: &str,
) -> egui::Response {
    let desired_size = Vec2::splat(28.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click());

    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        let bg = if selected {
            visuals.selection.bg_fill
        } else if response.hovered() {
            visuals.widgets.hovered.weak_bg_fill
        } else {
            Color32::TRANSPARENT
        };
        let fg = if selected {
            visuals.selection.stroke.color
        } else if response.hovered() {
            visuals.strong_text_color()
        } else {
            visuals.text_color()
        };

        ui.painter().rect_filled(rect, 6.0, bg);
        if selected {
            ui.painter().rect_stroke(
                rect,
                6.0,
                Stroke::new(1.0_f32, visuals.selection.stroke.color.linear_multiply(0.5)),
                egui::StrokeKind::Inside,
            );
        }
        paint_icon(ui.painter(), rect.shrink(5.0), icon, fg);
    }

    response.on_hover_text(tooltip)
}

/// Renders a compact button with a Heroicons icon + short text label.
pub fn icon_text_button(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    primary: bool,
) -> egui::Response {
    let font_id = egui::TextStyle::Button.resolve(ui.style());
    let galley = ui.painter().layout_no_wrap(label.to_string(), font_id, Color32::WHITE);
    let pad_x = 8.0;
    let icon_sz = 16.0;
    let gap = 5.0;
    let width = pad_x * 2.0 + icon_sz + gap + galley.size().x;
    let height = 28.0;

    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::click());

    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        let (bg, fg, border) = if primary {
            let base = Color32::from_rgb(37, 99, 235); // Tailwind blue-600
            let bg = if response.is_pointer_button_down_on() {
                Color32::from_rgb(29, 78, 216)
            } else if response.hovered() {
                Color32::from_rgb(59, 130, 246)
            } else {
                base
            };
            (bg, Color32::WHITE, Stroke::NONE)
        } else {
            let bg = if response.hovered() {
                visuals.widgets.hovered.weak_bg_fill
            } else {
                visuals.widgets.inactive.weak_bg_fill
            };
            let fg = visuals.strong_text_color();
            (bg, fg, Stroke::new(1.0_f32, visuals.widgets.noninteractive.bg_stroke.color))
        };

        ui.painter().rect_filled(rect, 6.0, bg);
        if border != Stroke::NONE {
            ui.painter().rect_stroke(rect, 6.0, border, egui::StrokeKind::Inside);
        }

        let icon_rect = Rect::from_min_size(
            Pos2::new(rect.min.x + pad_x, rect.center().y - icon_sz * 0.5),
            Vec2::splat(icon_sz),
        );
        paint_icon(ui.painter(), icon_rect, icon, fg);

        let text_pos = Pos2::new(icon_rect.max.x + gap, rect.center().y - galley.size().y * 0.5);
        ui.painter().text(
            text_pos,
            egui::Align2::LEFT_TOP,
            label,
            egui::TextStyle::Button.resolve(ui.style()),
            fg,
        );
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_has_a_valid_heroicons_svg() {
        for icon in Icon::ALL {
            let (uri, bytes) = icon.source();
            assert!(uri.starts_with("bytes://heroicons/") && uri.ends_with(".svg"));
            let text = std::str::from_utf8(bytes).unwrap();
            assert!(text.starts_with("<svg"), "{uri}");
            assert!(!text.contains("currentColor"), "{uri} must be pre-coloured white");
        }
    }

    #[test]
    fn app_icon_has_dark_tile_blue_corners_and_amber_bolt() {
        let size = 64;
        let rgba = app_icon_rgba(size);
        assert_eq!(rgba.len(), (size * size * 4) as usize);
        let px = |x: u32, y: u32| {
            let i = ((y * size + x) * 4) as usize;
            [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
        };
        // Outer corner is transparent (rounded tile), tile itself is opaque dark slate.
        assert_eq!(px(0, 0)[3], 0);
        let tile = px(32, 6);
        assert!(tile[3] == 255 && tile[0] < 40 && tile[2] < 60, "{tile:?}");
        // Centre of the bolt is amber (#f59e0b).
        let bolt = px(32, 32);
        assert!(bolt[0] > 200 && bolt[1] > 120 && bolt[2] < 60, "{bolt:?}");
        // Some pixels are the blue viewfinder stroke (#60a5fa).
        assert!(rgba.chunks_exact(4).any(|p| p[2] > 220 && p[0] < 130 && p[3] > 200));
    }
}
