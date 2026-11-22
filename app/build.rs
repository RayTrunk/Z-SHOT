// Z-SHOT — Copyright (C) 2026 R. Trunk
// SPDX-License-Identifier: GPL-3.0-or-later

//! Renders `assets/z-shot-icon.svg` into a multi-size `.ico` and embeds it as the
//! Windows executable icon (shown in Explorer, Start menu and shortcuts).

use std::path::PathBuf;

const SVG: &str = "assets/z-shot-icon.svg";
const SIZES: [u32; 6] = [16, 24, 32, 48, 64, 256];

fn main() {
    println!("cargo:rerun-if-changed={SVG}");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml"); // version for the file details

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let svg = std::fs::read_to_string(SVG).expect("read app icon SVG");

    let ico_path = out.join("z-shot.ico");
    std::fs::write(&ico_path, build_ico(&svg)).expect("write .ico");

    let rc_path = out.join("z-shot.rc");
    let ico = ico_path.display().to_string().replace('\\', "\\\\");
    let rc = format!("1 ICON \"{ico}\"\n\n{}", version_info());
    std::fs::write(&rc_path, rc).expect("write .rc");

    embed_resource::compile(&rc_path, embed_resource::NONE)
        .manifest_optional()
        .expect("embed icon resource");
}

/// Version, copyright and licence for Explorer's "Properties › Details" tab.
/// Plain ASCII on purpose: the resource compiler reads the file in the ANSI codepage.
fn version_info() -> String {
    let version = std::env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION");
    let mut parts = version
        .split(['.', '-', '+'])
        .map(|p| p.parse::<u16>().unwrap_or(0));
    let [major, minor, patch] = [(); 3].map(|_| parts.next().unwrap_or(0));
    format!(
        r#"1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040704B0"
    BEGIN
      VALUE "CompanyName", "R. Trunk"
      VALUE "FileDescription", "Z-SHOT Screenshot-Werkzeug"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "z-shot"
      VALUE "LegalCopyright", "Copyright (C) 2026 R. Trunk - GPL-3.0-or-later"
      VALUE "OriginalFilename", "z-shot.exe"
      VALUE "ProductName", "Z-SHOT"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x0407, 1200
  END
END
"#
    )
}

/// ICO container with one PNG-compressed 32-bit image per size.
fn build_ico(svg: &str) -> Vec<u8> {
    let pngs: Vec<Vec<u8>> = SIZES.iter().map(|&s| render_png(svg, s)).collect();

    let mut ico = Vec::new();
    ico.extend_from_slice(&[0, 0, 1, 0]);
    ico.extend_from_slice(&(SIZES.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * SIZES.len() as u32;
    for (&size, png) in SIZES.iter().zip(&pngs) {
        let dim = if size >= 256 { 0 } else { size as u8 };
        ico.extend_from_slice(&[dim, dim, 0, 0]);
        ico.extend_from_slice(&1u16.to_le_bytes()); // planes
        ico.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        ico.extend_from_slice(&(png.len() as u32).to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for png in &pngs {
        ico.extend_from_slice(png);
    }
    ico
}

fn render_png(svg: &str, size: u32) -> Vec<u8> {
    use resvg::{tiny_skia, usvg, usvg::TreeParsing};

    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).expect("parse icon SVG");
    let mut pixmap = tiny_skia::Pixmap::new(size, size).expect("pixmap");
    let scale = size as f32 / tree.size.width();
    resvg::Tree::from_usvg(&tree)
        .render(tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());

    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for px in pixmap.pixels() {
        let c = px.demultiply();
        rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    let img = image::RgbaImage::from_raw(size, size, rgba).expect("rgba buffer");
    let mut png = std::io::Cursor::new(Vec::new());
    img.write_to(&mut png, image::ImageFormat::Png).expect("encode png");
    png.into_inner()
}
