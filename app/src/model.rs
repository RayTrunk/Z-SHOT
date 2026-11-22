// Z-SHOT — Copyright (C) 2026 R. Trunk
// SPDX-License-Identifier: GPL-3.0-or-later

use eframe::egui::{self, Color32, Pos2, Rect, TextureHandle};
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tool {
    Select,
    Arrow,
    Rect,
    Highlight,
    Redact,
    Text,
    Freehand,
    Crop,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Annotation {
    Arrow {
        from: [f32; 2],
        to: [f32; 2],
        color: [u8; 4],
        width: f32,
    },
    Rect {
        min: [f32; 2],
        max: [f32; 2],
        color: [u8; 4],
        width: f32,
    },
    Highlight {
        min: [f32; 2],
        max: [f32; 2],
        color: [u8; 4],
    },
    Redact {
        min: [f32; 2],
        max: [f32; 2],
    },
    Freehand {
        points: Vec<[f32; 2]>,
        color: [u8; 4],
        width: f32,
    },
    Text {
        pos: [f32; 2],
        text: String,
        color: [u8; 4],
        size: f32,
    },
}

impl Annotation {
    pub fn hit_test(&self, p: Pos2) -> bool {
        match self {
            Annotation::Arrow { from, to, .. } => {
                dist_to_segment(p, Pos2::new(from[0], from[1]), Pos2::new(to[0], to[1])) < 14.0
            }
            Annotation::Rect { min, max, .. } => {
                let r = Rect::from_two_pos(Pos2::new(min[0], min[1]), Pos2::new(max[0], max[1]));
                r.expand(8.0).contains(p) && !r.shrink(10.0).contains(p)
            }
            Annotation::Highlight { min, max, .. } | Annotation::Redact { min, max } => {
                let r = Rect::from_two_pos(Pos2::new(min[0], min[1]), Pos2::new(max[0], max[1]));
                r.contains(p)
            }
            Annotation::Freehand { points, .. } => points
                .windows(2)
                .any(|w| dist_to_segment(p, Pos2::new(w[0][0], w[0][1]), Pos2::new(w[1][0], w[1][1])) < 12.0),
            Annotation::Text { pos, text, size, .. } => {
                let w = (text.chars().count() as f32 * size * 0.55).max(40.0);
                let r = Rect::from_min_size(Pos2::new(pos[0], pos[1]), eframe::egui::vec2(w, *size * 1.4));
                r.expand(6.0).contains(p)
            }
        }
    }
}

fn dist_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len_sq = ab.length_sq();
    if len_sq < 1e-4 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShotMeta {
    pub id: String,
    pub title: String,
    pub notes: String,
    pub created_at: String,
    pub favorite: bool,
    pub width: u32,
    pub height: u32,
    pub annotations: Vec<Annotation>,
}

pub struct ShotItem {
    pub meta: ShotMeta,
    pub image: RgbaImage,
    pub texture: Option<TextureHandle>,
    pub undo_stack: Vec<Vec<Annotation>>,
    pub redo_stack: Vec<Vec<Annotation>>,
}

impl ShotItem {
    pub fn new(id: String, title: String, notes: String, image: RgbaImage) -> Self {
        let now = chrono::Local::now().format("%d.%m.%Y %H:%M").to_string();
        let meta = ShotMeta {
            id,
            title,
            notes,
            created_at: now,
            favorite: false,
            width: image.width(),
            height: image.height(),
            annotations: Vec::new(),
        };
        Self {
            meta,
            image,
            texture: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// False while the image is still being decoded in the background after start-up
    /// (see [`spawn_album_image_loader`]); `meta.width`/`meta.height` are valid anyway.
    pub fn is_loaded(&self) -> bool {
        self.image.width() > 0
    }

    pub fn push_annotation(&mut self, ann: Annotation) {
        self.undo_stack.push(self.meta.annotations.clone());
        self.redo_stack.clear();
        self.meta.annotations.push(ann);
    }

    pub fn delete_annotation(&mut self, idx: usize) {
        if idx < self.meta.annotations.len() {
            self.undo_stack.push(self.meta.annotations.clone());
            self.redo_stack.clear();
            self.meta.annotations.remove(idx);
        }
    }

    pub fn undo(&mut self) -> bool {
        if let Some(prev) = self.undo_stack.pop() {
            self.redo_stack.push(self.meta.annotations.clone());
            self.meta.annotations = prev;
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(self.meta.annotations.clone());
            self.meta.annotations = next;
            true
        } else {
            false
        }
    }

    pub fn apply_crop(&mut self, min_x: u32, min_y: u32, max_x: u32, max_y: u32) -> bool {
        let x0 = min_x.min(self.image.width().saturating_sub(1));
        let y0 = min_y.min(self.image.height().saturating_sub(1));
        let x1 = max_x.min(self.image.width());
        let y1 = max_y.min(self.image.height());
        if x1 <= x0 + 8 || y1 <= y0 + 8 {
            return false;
        }
        let w = x1 - x0;
        let h = y1 - y0;
        let cropped = image::imageops::crop_imm(&self.image, x0, y0, w, h).to_image();
        self.image = cropped;
        self.meta.width = w;
        self.meta.height = h;
        self.texture = None;

        // Shift annotations by crop offset
        let dx = x0 as f32;
        let dy = y0 as f32;
        for ann in &mut self.meta.annotations {
            match ann {
                Annotation::Arrow { from, to, .. } => {
                    from[0] -= dx;
                    from[1] -= dy;
                    to[0] -= dx;
                    to[1] -= dy;
                }
                Annotation::Rect { min, max, .. }
                | Annotation::Highlight { min, max, .. }
                | Annotation::Redact { min, max } => {
                    min[0] -= dx;
                    min[1] -= dy;
                    max[0] -= dx;
                    max[1] -= dy;
                }
                Annotation::Freehand { points, .. } => {
                    for p in points {
                        p[0] -= dx;
                        p[1] -= dy;
                    }
                }
                Annotation::Text { pos, .. } => {
                    pos[0] -= dx;
                    pos[1] -= dy;
                }
            }
        }
        true
    }

    pub fn matches_query(&self, query: &str, only_favorites: bool) -> bool {
        if only_favorites && !self.meta.favorite {
            return false;
        }
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return true;
        }
        self.meta.title.to_lowercase().contains(&q)
            || self.meta.notes.to_lowercase().contains(&q)
            || self.meta.created_at.to_lowercase().contains(&q)
            || self.meta.annotations.iter().any(|a| match a {
                Annotation::Text { text, .. } => text.to_lowercase().contains(&q),
                _ => false,
            })
    }

    /// Renders the base image plus all annotations into a new RGBA image for export/clipboard.
    pub fn render_flattened(&self) -> RgbaImage {
        let mut out = self.image.clone();
        for ann in &self.meta.annotations {
            match ann {
                Annotation::Redact { min, max } => {
                    let x0 = min[0].min(max[0]).max(0.0) as u32;
                    let y0 = min[1].min(max[1]).max(0.0) as u32;
                    let x1 = min[0].max(max[0]).min(out.width() as f32) as u32;
                    let y1 = min[1].max(max[1]).min(out.height() as f32) as u32;
                    for y in y0..y1 {
                        for x in x0..x1 {
                            out.put_pixel(x, y, Rgba([15, 23, 42, 255]));
                        }
                    }
                }
                Annotation::Highlight { min, max, color } => {
                    let x0 = min[0].min(max[0]).max(0.0) as u32;
                    let y0 = min[1].min(max[1]).max(0.0) as u32;
                    let x1 = min[0].max(max[0]).min(out.width() as f32) as u32;
                    let y1 = min[1].max(max[1]).min(out.height() as f32) as u32;
                    let alpha = 0.35f32;
                    for y in y0..y1 {
                        for x in x0..x1 {
                            let dst = out.get_pixel(x, y).0;
                            let r = (dst[0] as f32 * (1.0 - alpha) + color[0] as f32 * alpha) as u8;
                            let g = (dst[1] as f32 * (1.0 - alpha) + color[1] as f32 * alpha) as u8;
                            let b = (dst[2] as f32 * (1.0 - alpha) + color[2] as f32 * alpha) as u8;
                            out.put_pixel(x, y, Rgba([r, g, b, 255]));
                        }
                    }
                }
                Annotation::Rect { min, max, color, width } => {
                    let x0 = min[0].min(max[0]);
                    let y0 = min[1].min(max[1]);
                    let x1 = min[0].max(max[0]);
                    let y1 = min[1].max(max[1]);
                    let c = Rgba(*color);
                    draw_thick_line(&mut out, x0, y0, x1, y0, *width, c);
                    draw_thick_line(&mut out, x1, y0, x1, y1, *width, c);
                    draw_thick_line(&mut out, x1, y1, x0, y1, *width, c);
                    draw_thick_line(&mut out, x0, y1, x0, y0, *width, c);
                }
                Annotation::Arrow { from, to, color, width } => {
                    let c = Rgba(*color);
                    draw_thick_line(&mut out, from[0], from[1], to[0], to[1], *width, c);
                    let dx = to[0] - from[0];
                    let dy = to[1] - from[1];
                    let len = (dx * dx + dy * dy).sqrt().max(1.0);
                    let ux = dx / len;
                    let uy = dy / len;
                    let head = (width * 4.2).max(14.0);
                    let left_x = to[0] - ux * head - uy * head * 0.55;
                    let left_y = to[1] - uy * head + ux * head * 0.55;
                    let right_x = to[0] - ux * head + uy * head * 0.55;
                    let right_y = to[1] - uy * head - ux * head * 0.55;
                    draw_thick_line(&mut out, to[0], to[1], left_x, left_y, *width, c);
                    draw_thick_line(&mut out, to[0], to[1], right_x, right_y, *width, c);
                }
                Annotation::Freehand { points, color, width } => {
                    let c = Rgba(*color);
                    for w in points.windows(2) {
                        draw_thick_line(&mut out, w[0][0], w[0][1], w[1][0], w[1][1], *width, c);
                    }
                }
                Annotation::Text { pos, text, color, size } => {
                    // Draw a subtle tag badge in exported raster if text is present
                    if !text.is_empty() {
                        let w = (text.chars().count() as f32 * size * 0.52).max(24.0);
                        let h = *size * 1.25;
                        let x0 = pos[0].max(0.0);
                        let y0 = pos[1].max(0.0);
                        draw_thick_line(&mut out, x0, y0 + h, x0 + w, y0 + h, 2.5, Rgba(*color));
                    }
                }
            }
        }
        out
    }
}

fn draw_thick_line(img: &mut RgbaImage, x0: f32, y0: f32, x1: f32, y1: f32, width: f32, color: Rgba<u8>) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let steps = dx.abs().max(dy.abs()).ceil().max(1.0) as i32;
    let radius = (width * 0.5).max(1.0);
    let r_ceil = radius.ceil() as i32;
    let r2 = radius * radius;
    let w = img.width() as i32;
    let h = img.height() as i32;

    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let cx = x0 + dx * t;
        let cy = y0 + dy * t;
        let cxi = cx.round() as i32;
        let cyi = cy.round() as i32;
        for oy in -r_ceil..=r_ceil {
            for ox in -r_ceil..=r_ceil {
                if (ox * ox + oy * oy) as f32 <= r2 {
                    let px = cxi + ox;
                    let py = cyi + oy;
                    if px >= 0 && py >= 0 && px < w && py < h {
                        img.put_pixel(px as u32, py as u32, color);
                    }
                }
            }
        }
    }
}

pub fn color32_to_arr(c: Color32) -> [u8; 4] {
    [c.r(), c.g(), c.b(), c.a()]
}

pub fn arr_to_color32(a: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(a[0], a[1], a[2], a[3])
}

/// Returns the local album directory (`%LOCALAPPDATA%\Z-SHOT` on Windows).
pub fn album_dir() -> PathBuf {
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        PathBuf::from(local).join("Z-SHOT")
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".z-shot")
    } else {
        PathBuf::from("album")
    }
}

/// Writes only `album.json`. The images are written where they change (capture, crop).
pub fn save_album_index(dir: &Path, items: &[ShotItem]) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let metas: Vec<&ShotMeta> = items.iter().map(|i| &i.meta).collect();
    let json = serde_json::to_string_pretty(&metas).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("album.json"), json).map_err(|e| e.to_string())
}

/// Writes `album.json` plus every image that is not on disk yet.
pub fn save_album(dir: &Path, items: &[ShotItem]) -> Result<(), String> {
    save_album_index(dir, items)?;
    for item in items {
        let png_path = item_image_path(dir, &item.meta.id);
        if !png_path.exists() {
            item.image.save(&png_path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Where an album item's image lives.
pub fn item_image_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.png"))
}

/// Reads `album.json` without decoding any image: decoding 60 screenshots blocked the
/// start for 10 s in a debug build, and hotkeys pressed meanwhile went unanswered.
/// The items start with an empty image; [`spawn_album_image_loader`] fills them in.
pub fn load_album_index(dir: &Path) -> Vec<ShotItem> {
    let index_path = dir.join("album.json");
    let Ok(raw) = std::fs::read_to_string(index_path) else {
        return Vec::new();
    };
    let Ok(metas) = serde_json::from_str::<Vec<ShotMeta>>(&raw) else {
        return Vec::new();
    };
    metas
        .into_iter()
        .filter(|meta| item_image_path(dir, &meta.id).exists())
        .map(|meta| ShotItem {
            meta,
            image: RgbaImage::new(0, 0),
            texture: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        })
        .collect()
}

/// Decodes album images on a worker thread in the given order (newest first) and sends
/// each one as soon as it is ready, `None` if it could not be read.
pub fn spawn_album_image_loader(
    dir: PathBuf,
    ids: Vec<String>,
    ctx: egui::Context,
) -> std::sync::mpsc::Receiver<(String, Option<RgbaImage>)> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for id in ids {
            let image = image::open(item_image_path(&dir, &id)).ok().map(|img| img.to_rgba8());
            if tx.send((id, image)).is_err() {
                return;
            }
            ctx.request_repaint();
        }
    });
    rx
}
