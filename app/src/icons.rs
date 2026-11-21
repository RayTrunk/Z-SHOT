use eframe::egui::{self, Color32, Painter, Pos2, Rect, Stroke, Vec2};

/// Crisp 24x24 Heroicons-inspired outline icons rendered directly with egui Painter.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Camera,
    Desktop,
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

/// Paint an icon inside `rect` using a 24x24 coordinate space (matching Heroicons).
pub fn paint_icon(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let s = rect.width().min(rect.height()) / 24.0;
    let origin = rect.center() - Vec2::splat(12.0 * s);
    let pt = |x: f32, y: f32| Pos2::new(origin.x + x * s, origin.y + y * s);
    let stroke = Stroke::new((1.75 * s).max(1.2), color);
    let thin = Stroke::new((1.4 * s).max(1.0), color);

    match icon {
        Icon::Camera => {
            painter.rect_stroke(
                Rect::from_min_max(pt(3.0, 7.0), pt(21.0, 19.0)),
                3.0 * s,
                stroke,
                egui::StrokeKind::Middle,
            );
            painter.line_segment([pt(7.5, 7.0), pt(9.0, 4.5)], stroke);
            painter.line_segment([pt(9.0, 4.5), pt(15.0, 4.5)], stroke);
            painter.line_segment([pt(15.0, 4.5), pt(16.5, 7.0)], stroke);
            painter.circle_stroke(pt(12.0, 13.0), 3.3 * s, stroke);
        }
        Icon::Desktop => {
            painter.rect_stroke(
                Rect::from_min_max(pt(3.0, 4.0), pt(21.0, 16.5)),
                2.0 * s,
                stroke,
                egui::StrokeKind::Middle,
            );
            painter.line_segment([pt(9.0, 20.0), pt(15.0, 20.0)], stroke);
            painter.line_segment([pt(12.0, 16.5), pt(12.0, 20.0)], stroke);
        }
        Icon::Crop => {
            // Viewfinder / crop corners
            painter.line_segment([pt(6.5, 3.0), pt(6.5, 17.5)], stroke);
            painter.line_segment([pt(6.5, 17.5), pt(21.0, 17.5)], stroke);
            painter.line_segment([pt(3.0, 6.5), pt(17.5, 6.5)], stroke);
            painter.line_segment([pt(17.5, 6.5), pt(17.5, 21.0)], stroke);
        }
        Icon::Clock => {
            painter.circle_stroke(pt(12.0, 12.0), 8.5 * s, stroke);
            painter.line_segment([pt(12.0, 7.0), pt(12.0, 12.0)], stroke);
            painter.line_segment([pt(12.0, 12.0), pt(15.5, 14.0)], stroke);
        }
        Icon::Cursor => {
            let pts = vec![
                pt(5.5, 4.0),
                pt(5.5, 18.5),
                pt(9.5, 14.8),
                pt(12.8, 20.0),
                pt(15.0, 18.8),
                pt(11.8, 13.6),
                pt(17.5, 13.6),
                pt(5.5, 4.0),
            ];
            painter.add(egui::Shape::line(pts, stroke));
        }
        Icon::ArrowUpRight => {
            painter.line_segment([pt(5.5, 18.5), pt(18.5, 5.5)], stroke);
            painter.line_segment([pt(9.5, 5.5), pt(18.5, 5.5)], stroke);
            painter.line_segment([pt(18.5, 5.5), pt(18.5, 14.5)], stroke);
        }
        Icon::Square => {
            painter.rect_stroke(
                Rect::from_min_max(pt(4.5, 4.5), pt(19.5, 19.5)),
                2.5 * s,
                stroke,
                egui::StrokeKind::Middle,
            );
        }
        Icon::Highlighter => {
            // Chisel tip marker
            let pts = vec![
                pt(15.0, 4.0),
                pt(20.0, 9.0),
                pt(10.5, 18.5),
                pt(5.5, 18.5),
                pt(5.5, 13.5),
                pt(15.0, 4.0),
            ];
            painter.add(egui::Shape::line(pts, stroke));
            painter.line_segment([pt(4.0, 20.5), pt(16.0, 20.5)], stroke);
        }
        Icon::EyeSlash => {
            // Opaque redaction / privacy icon
            painter.line_segment([pt(3.5, 12.0), pt(7.5, 8.2)], thin);
            painter.line_segment([pt(7.5, 8.2), pt(12.0, 7.0)], thin);
            painter.line_segment([pt(12.0, 7.0), pt(16.5, 8.2)], thin);
            painter.line_segment([pt(16.5, 8.2), pt(20.5, 12.0)], thin);
            painter.line_segment([pt(3.5, 12.0), pt(7.5, 15.8)], thin);
            painter.line_segment([pt(7.5, 15.8), pt(12.0, 17.0)], thin);
            painter.line_segment([pt(12.0, 17.0), pt(16.5, 15.8)], thin);
            painter.line_segment([pt(16.5, 15.8), pt(20.5, 12.0)], thin);
            painter.circle_stroke(pt(12.0, 12.0), 2.5 * s, thin);
            painter.line_segment([pt(4.0, 4.0), pt(20.0, 20.0)], stroke);
        }
        Icon::Text => {
            painter.line_segment([pt(5.0, 5.5), pt(19.0, 5.5)], stroke);
            painter.line_segment([pt(12.0, 5.5), pt(12.0, 19.0)], stroke);
            painter.line_segment([pt(8.5, 19.0), pt(15.5, 19.0)], stroke);
        }
        Icon::Pencil => {
            let pts = vec![
                pt(16.5, 4.5),
                pt(19.5, 7.5),
                pt(8.5, 18.5),
                pt(4.5, 19.5),
                pt(5.5, 15.5),
                pt(16.5, 4.5),
            ];
            painter.add(egui::Shape::line(pts, stroke));
        }
        Icon::Undo => {
            painter.line_segment([pt(9.0, 7.0), pt(5.0, 10.5)], stroke);
            painter.line_segment([pt(9.0, 14.0), pt(5.0, 10.5)], stroke);
            let pts = vec![
                pt(5.5, 10.5),
                pt(14.5, 10.5),
                pt(17.5, 11.5),
                pt(18.5, 14.5),
                pt(17.5, 17.5),
                pt(14.5, 18.5),
                pt(10.0, 18.5),
            ];
            painter.add(egui::Shape::line(pts, stroke));
        }
        Icon::Redo => {
            painter.line_segment([pt(15.0, 7.0), pt(19.0, 10.5)], stroke);
            painter.line_segment([pt(15.0, 14.0), pt(19.0, 10.5)], stroke);
            let pts = vec![
                pt(18.5, 10.5),
                pt(9.5, 10.5),
                pt(6.5, 11.5),
                pt(5.5, 14.5),
                pt(6.5, 17.5),
                pt(9.5, 18.5),
                pt(14.0, 18.5),
            ];
            painter.add(egui::Shape::line(pts, stroke));
        }
        Icon::Clipboard => {
            painter.rect_stroke(
                Rect::from_min_max(pt(5.5, 5.5), pt(18.5, 20.5)),
                2.0 * s,
                stroke,
                egui::StrokeKind::Middle,
            );
            painter.rect_stroke(
                Rect::from_min_max(pt(9.0, 3.5), pt(15.0, 7.0)),
                1.2 * s,
                stroke,
                egui::StrokeKind::Middle,
            );
            painter.line_segment([pt(8.5, 11.5), pt(15.5, 11.5)], thin);
            painter.line_segment([pt(8.5, 15.5), pt(13.5, 15.5)], thin);
        }
        Icon::Download => {
            painter.line_segment([pt(12.0, 4.0), pt(12.0, 15.0)], stroke);
            painter.line_segment([pt(8.0, 11.5), pt(12.0, 15.5)], stroke);
            painter.line_segment([pt(16.0, 11.5), pt(12.0, 15.5)], stroke);
            let pts = vec![pt(4.5, 16.0), pt(4.5, 19.5), pt(19.5, 19.5), pt(19.5, 16.0)];
            painter.add(egui::Shape::line(pts, stroke));
        }
        Icon::Trash => {
            painter.line_segment([pt(4.5, 6.5), pt(19.5, 6.5)], stroke);
            painter.line_segment([pt(9.0, 4.0), pt(15.0, 4.0)], stroke);
            let pts = vec![pt(6.5, 6.5), pt(7.5, 19.5), pt(16.5, 19.5), pt(17.5, 6.5)];
            painter.add(egui::Shape::line(pts, stroke));
            painter.line_segment([pt(10.0, 10.0), pt(10.0, 16.0)], thin);
            painter.line_segment([pt(14.0, 10.0), pt(14.0, 16.0)], thin);
        }
        Icon::Star | Icon::StarSolid => {
            let star = vec![
                pt(12.0, 3.5),
                pt(14.6, 8.8),
                pt(20.4, 9.6),
                pt(16.2, 13.7),
                pt(17.2, 19.5),
                pt(12.0, 16.7),
                pt(6.8, 19.5),
                pt(7.8, 13.7),
                pt(3.6, 9.6),
                pt(9.4, 8.8),
            ];
            if matches!(icon, Icon::StarSolid) {
                painter.add(egui::Shape::convex_polygon(star.clone(), color, Stroke::NONE));
            }
            let mut closed = star;
            closed.push(pt(12.0, 3.5));
            painter.add(egui::Shape::line(closed, stroke));
        }
        Icon::Search => {
            painter.circle_stroke(pt(10.5, 10.5), 6.0 * s, stroke);
            painter.line_segment([pt(15.0, 15.0), pt(20.0, 20.0)], stroke);
        }
        Icon::Sun => {
            painter.circle_stroke(pt(12.0, 12.0), 4.0 * s, stroke);
            for i in 0..8 {
                let a = (i as f32) * std::f32::consts::FRAC_PI_4;
                let (c, si) = (a.cos(), a.sin());
                painter.line_segment(
                    [pt(12.0 + c * 6.5, 12.0 + si * 6.5), pt(12.0 + c * 9.0, 12.0 + si * 9.0)],
                    thin,
                );
            }
        }
        Icon::Moon => {
            let pts = vec![
                pt(15.0, 4.5),
                pt(10.5, 5.5),
                pt(7.0, 8.5),
                pt(6.0, 13.0),
                pt(8.0, 17.0),
                pt(12.0, 19.0),
                pt(16.5, 18.5),
                pt(19.5, 16.0),
                pt(15.0, 15.5),
                pt(12.0, 13.0),
                pt(11.5, 9.0),
                pt(15.0, 4.5),
            ];
            painter.add(egui::Shape::line(pts, stroke));
        }
        Icon::ZoomIn => {
            painter.circle_stroke(pt(10.5, 10.5), 6.0 * s, stroke);
            painter.line_segment([pt(15.0, 15.0), pt(20.0, 20.0)], stroke);
            painter.line_segment([pt(8.0, 10.5), pt(13.0, 10.5)], thin);
            painter.line_segment([pt(10.5, 8.0), pt(10.5, 13.0)], thin);
        }
        Icon::ZoomOut => {
            painter.circle_stroke(pt(10.5, 10.5), 6.0 * s, stroke);
            painter.line_segment([pt(15.0, 15.0), pt(20.0, 20.0)], stroke);
            painter.line_segment([pt(8.0, 10.5), pt(13.0, 10.5)], thin);
        }
        Icon::FitScreen => {
            painter.line_segment([pt(4.5, 9.0), pt(4.5, 4.5)], stroke);
            painter.line_segment([pt(4.5, 4.5), pt(9.0, 4.5)], stroke);
            painter.line_segment([pt(15.0, 4.5), pt(19.5, 4.5)], stroke);
            painter.line_segment([pt(19.5, 4.5), pt(19.5, 9.0)], stroke);
            painter.line_segment([pt(19.5, 15.0), pt(19.5, 19.5)], stroke);
            painter.line_segment([pt(19.5, 19.5), pt(15.0, 19.5)], stroke);
            painter.line_segment([pt(9.0, 19.5), pt(4.5, 19.5)], stroke);
            painter.line_segment([pt(4.5, 19.5), pt(4.5, 15.0)], stroke);
        }
        Icon::Sidebar => {
            painter.rect_stroke(
                Rect::from_min_max(pt(3.5, 4.5), pt(20.5, 19.5)),
                2.0 * s,
                stroke,
                egui::StrokeKind::Middle,
            );
            painter.line_segment([pt(9.0, 4.5), pt(9.0, 19.5)], stroke);
        }
        Icon::Check => {
            painter.line_segment([pt(5.0, 12.5), pt(10.0, 17.5)], stroke);
            painter.line_segment([pt(10.0, 17.5), pt(19.0, 6.5)], stroke);
        }
        Icon::XMark => {
            painter.line_segment([pt(6.0, 6.0), pt(18.0, 18.0)], stroke);
            painter.line_segment([pt(18.0, 6.0), pt(6.0, 18.0)], stroke);
        }
        Icon::Background => {
            // Tray / minimize-to-background icon
            let tray = vec![pt(4.5, 14.5), pt(4.5, 19.5), pt(19.5, 19.5), pt(19.5, 14.5)];
            painter.add(egui::Shape::line(tray, stroke));
            painter.line_segment([pt(12.0, 4.5), pt(12.0, 14.0)], stroke);
            painter.line_segment([pt(8.5, 10.5), pt(12.0, 14.0)], stroke);
            painter.line_segment([pt(15.5, 10.5), pt(12.0, 14.0)], stroke);
        }
        Icon::Power => {
            painter.line_segment([pt(12.0, 4.0), pt(12.0, 11.5)], stroke);
            let mut arc = Vec::new();
            for step in 0..=16 {
                let a = -0.35 * std::f32::consts::PI
                    + (step as f32 / 16.0) * (1.70 * std::f32::consts::PI);
                arc.push(pt(12.0 + a.cos() * 7.2, 12.5 + a.sin() * 7.2));
            }
            painter.add(egui::Shape::line(arc, stroke));
        }
        Icon::Cog => {
            // Gear: ring with 8 teeth and a hub
            painter.circle_stroke(pt(12.0, 12.0), 6.0 * s, stroke);
            painter.circle_stroke(pt(12.0, 12.0), 2.4 * s, thin);
            let tooth = Stroke::new((2.6 * s).max(1.6), color);
            for i in 0..8 {
                let a = (i as f32) * std::f32::consts::FRAC_PI_4 + std::f32::consts::PI / 8.0;
                let (c, si) = (a.cos(), a.sin());
                painter.line_segment(
                    [pt(12.0 + c * 6.0, 12.0 + si * 6.0), pt(12.0 + c * 8.8, 12.0 + si * 8.8)],
                    tooth,
                );
            }
        }
    }
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
        paint_icon(ui.painter(), rect.shrink(5.5), icon, fg);
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
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        font_id,
        Color32::WHITE,
    );
    let pad_x = 8.0;
    let icon_sz = 15.0;
    let gap = 5.0;
    let width = pad_x * 2.0 + icon_sz + gap + galley.size().x;
    let height = 28.0;

    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::click());

    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        let (bg, fg, border) = if primary {
            let base = Color32::from_rgb(37, 99, 235); // Clean Tailwind blue-600
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

        let text_pos = Pos2::new(
            icon_rect.max.x + gap,
            rect.center().y - galley.size().y * 0.5,
        );
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
