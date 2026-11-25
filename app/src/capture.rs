// Z-SHOT — Copyright (C) 2026 R. Trunk
// SPDX-License-Identifier: GPL-3.0-or-later

use image::{Rgba, RgbaImage};

/// Captures the primary desktop monitor on Windows using Win32 GDI.
/// Falls back to a procedural canvas if capture fails or on non-Windows platforms.
#[cfg(windows)]
pub fn capture_primary_screen() -> Result<RgbaImage, String> {
    use windows_sys::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        SRCCOPY,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

    unsafe {
        let width = GetSystemMetrics(SM_CXSCREEN);
        let height = GetSystemMetrics(SM_CYSCREEN);
        if width <= 0 || height <= 0 {
            return Err(crate::i18n::tr("Bildschirmgröße konnte nicht ermittelt werden", "Could not determine the screen size").into());
        }

        let hdc_screen = GetDC(std::ptr::null_mut());
        if hdc_screen.is_null() {
            return Err(crate::i18n::tr("GetDC fehlgeschlagen", "GetDC failed").into());
        }
        let hdc_mem = CreateCompatibleDC(hdc_screen);
        if hdc_mem.is_null() {
            ReleaseDC(std::ptr::null_mut(), hdc_screen);
            return Err(crate::i18n::tr("CreateCompatibleDC fehlgeschlagen", "CreateCompatibleDC failed").into());
        }
        let hbm = CreateCompatibleBitmap(hdc_screen, width, height);
        if hbm.is_null() {
            DeleteDC(hdc_mem);
            ReleaseDC(std::ptr::null_mut(), hdc_screen);
            return Err(crate::i18n::tr("CreateCompatibleBitmap fehlgeschlagen", "CreateCompatibleBitmap failed").into());
        }

        let old = SelectObject(hdc_mem, hbm);
        let ok = BitBlt(hdc_mem, 0, 0, width, height, hdc_screen, 0, 0, SRCCOPY);

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height, // top-down DIB
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [std::mem::zeroed()],
        };

        let mut buf = vec![0u8; (width as usize) * (height as usize) * 4];
        let lines = if ok != 0 {
            GetDIBits(
                hdc_mem,
                hbm,
                0,
                height as u32,
                buf.as_mut_ptr() as *mut _,
                &mut bmi,
                DIB_RGB_COLORS,
            )
        } else {
            0
        };

        SelectObject(hdc_mem, old);
        DeleteObject(hbm);
        DeleteDC(hdc_mem);
        ReleaseDC(std::ptr::null_mut(), hdc_screen);

        if lines == 0 {
            return Err(crate::i18n::tr("Bildschirmaufnahme (GetDIBits) fehlgeschlagen", "Screen capture (GetDIBits) failed").into());
        }

        // Convert BGRA -> RGBA with full alpha
        for px in buf.chunks_exact_mut(4) {
            px.swap(0, 2);
            px[3] = 255;
        }

        RgbaImage::from_raw(width as u32, height as u32, buf)
            .ok_or_else(|| crate::i18n::tr("Bildpuffer ungültig", "Invalid image buffer").into())
    }
}

/// Linux: the whole desktop through the Screenshot portal, usually taken before the
/// window opened (see `linux::portal::precapture`).
#[cfg(target_os = "linux")]
pub fn capture_primary_screen() -> Result<RgbaImage, String> {
    crate::linux::portal::capture_screen()
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn capture_primary_screen() -> Result<RgbaImage, String> {
    Ok(generate_sample_capture(0))
}

/// Generates clean, realistic sample captures so a fresh album is immediately useful and testable.
pub fn generate_sample_capture(variant: usize) -> RgbaImage {
    let w = 960u32;
    let h = 580u32;
    let mut img = RgbaImage::new(w, h);

    if variant % 2 == 0 {
        // Sample 1: Clean light analytics / app window mockup
        fill_rect(&mut img, 0, 0, w, h, Rgba([241, 245, 249, 255]));
        // Title bar
        fill_rect(&mut img, 0, 0, w, 44, Rgba([30, 41, 59, 255]));
        fill_circle(&mut img, 22, 22, 6, Rgba([239, 68, 68, 255]));
        fill_circle(&mut img, 42, 22, 6, Rgba([245, 158, 11, 255]));
        fill_circle(&mut img, 62, 22, 6, Rgba([34, 197, 94, 255]));
        fill_rect(&mut img, 240, 12, 480, 20, Rgba([51, 65, 85, 255]));

        // Left nav panel
        fill_rect(&mut img, 24, 68, 190, 488, Rgba([255, 255, 255, 255]));
        stroke_rect(&mut img, 24, 68, 190, 488, Rgba([226, 232, 240, 255]));
        for i in 0..6 {
            let y = 94 + i * 44;
            let col = if i == 1 {
                Rgba([219, 234, 254, 255])
            } else {
                Rgba([241, 245, 249, 255])
            };
            fill_rect(&mut img, 40, y, 158, 28, col);
        }

        // Top KPI cards
        let card_colors = [
            Rgba([59, 130, 246, 255]),
            Rgba([16, 185, 129, 255]),
            Rgba([245, 158, 11, 255]),
        ];
        for (i, accent) in card_colors.iter().enumerate() {
            let x = 236 + (i as u32) * 238;
            fill_rect(&mut img, x, 68, 220, 118, Rgba([255, 255, 255, 255]));
            stroke_rect(&mut img, x, 68, 220, 118, Rgba([226, 232, 240, 255]));
            fill_rect(&mut img, x, 68, 220, 6, *accent);
            fill_rect(&mut img, x + 20, 96, 90, 14, Rgba([203, 213, 225, 255]));
            fill_rect(&mut img, x + 20, 126, 130, 32, Rgba([30, 41, 59, 255]));
        }

        // Chart card
        fill_rect(&mut img, 236, 208, 696, 348, Rgba([255, 255, 255, 255]));
        stroke_rect(&mut img, 236, 208, 696, 348, Rgba([226, 232, 240, 255]));
        // Grid lines
        for row in 0..5 {
            let y = 260 + row * 56;
            fill_rect(&mut img, 270, y, 630, 1, Rgba([226, 232, 240, 255]));
        }
        // Bar chart columns
        let heights = [120u32, 185, 150, 230, 195, 255, 170, 210, 240];
        for (i, &bh) in heights.iter().enumerate() {
            let x = 290 + (i as u32) * 66;
            let y = 510 - bh;
            let bar_col = if i == 5 {
                Rgba([37, 99, 235, 255])
            } else {
                Rgba([147, 197, 253, 255])
            };
            fill_rect(&mut img, x, y, 38, bh, bar_col);
        }
    } else {
        // Sample 2: Clean dark editor / notes window mockup
        fill_rect(&mut img, 0, 0, w, h, Rgba([15, 23, 42, 255]));
        fill_rect(&mut img, 0, 0, w, 40, Rgba([30, 41, 59, 255]));
        fill_circle(&mut img, 22, 20, 6, Rgba([239, 68, 68, 255]));
        fill_circle(&mut img, 42, 20, 6, Rgba([245, 158, 11, 255]));
        fill_circle(&mut img, 62, 20, 6, Rgba([34, 197, 94, 255]));

        // Code-like syntax bars
        let lines: &[(u32, u32, Rgba<u8>)] = &[
            (60, 180, Rgba([56, 189, 248, 255])),
            (84, 260, Rgba([167, 139, 250, 255])),
            (84, 340, Rgba([52, 211, 153, 255])),
            (108, 220, Rgba([251, 191, 36, 255])),
            (108, 390, Rgba([148, 163, 184, 255])),
            (84, 140, Rgba([167, 139, 250, 255])),
            (60, 310, Rgba([56, 189, 248, 255])),
            (84, 420, Rgba([248, 113, 113, 255])),
            (84, 280, Rgba([52, 211, 153, 255])),
            (60, 160, Rgba([148, 163, 184, 255])),
        ];
        for (i, &(indent, len, col)) in lines.iter().enumerate() {
            let y = 82 + (i as u32) * 38;
            fill_rect(&mut img, 28, y + 2, 18, 12, Rgba([71, 85, 105, 255]));
            fill_rect(&mut img, indent, y, len, 16, col);
        }

        // Right preview panel
        fill_rect(&mut img, 560, 64, 372, 488, Rgba([30, 41, 59, 255]));
        stroke_rect(&mut img, 560, 64, 372, 488, Rgba([51, 65, 85, 255]));
        fill_rect(&mut img, 588, 96, 316, 160, Rgba([51, 65, 85, 255]));
        fill_rect(&mut img, 588, 284, 220, 18, Rgba([96, 165, 250, 255]));
        fill_rect(&mut img, 588, 318, 300, 12, Rgba([148, 163, 184, 255]));
        fill_rect(&mut img, 588, 344, 260, 12, Rgba([148, 163, 184, 255]));
        fill_rect(&mut img, 588, 370, 280, 12, Rgba([148, 163, 184, 255]));
    }

    img
}

fn fill_rect(img: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, color: Rgba<u8>) {
    let max_x = (x + w).min(img.width());
    let max_y = (y + h).min(img.height());
    for py in y..max_y {
        for px in x..max_x {
            img.put_pixel(px, py, color);
        }
    }
}

fn stroke_rect(img: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, color: Rgba<u8>) {
    if w == 0 || h == 0 {
        return;
    }
    fill_rect(img, x, y, w, 1, color);
    fill_rect(img, x, y + h - 1, w, 1, color);
    fill_rect(img, x, y, 1, h, color);
    fill_rect(img, x + w - 1, y, 1, h, color);
}

fn fill_circle(img: &mut RgbaImage, cx: i32, cy: i32, r: i32, color: Rgba<u8>) {
    let r2 = r * r;
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r2 {
                let px = cx + dx;
                let py = cy + dy;
                if px >= 0 && py >= 0 && (px as u32) < img.width() && (py as u32) < img.height() {
                    img.put_pixel(px as u32, py as u32, color);
                }
            }
        }
    }
}
