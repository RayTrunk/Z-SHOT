# Z-SHOT — Mechanics & Configuration

Z-SHOT is a screenshot tool with a simple graphical interface in Rust + egui, Segoe UI typography, and Heroicons-style vector icons.

## Background & Capture Settings
- **Startup mode (Windows)**: Runs quietly in the background with a system tray icon and notification (user request)
- **Global hotkeys**: `Print Screen` (`Druck`) and `Ctrl+Shift+S` trigger fullscreen region-selection overlay from any app (user request)
- **Region capture flow**: Freezes desktop, lets user drag a marked area with live pixel size, then opens the full Editor GUI with the cropped screenshot (user request)
- **Full-screen capture**: Primary Windows monitor via Win32 GDI `BitBlt` (estimated)
- **Delayed capture countdown**: `3.0 s` before entering region-selection overlay (estimated)
- **Close window behavior**: Minimizes back to background; explicit Quit button or tray menu exits cleanly (user request)

## Canvas & Controls
- **Zoom range**: `0.25x` to `4.0x` (estimated)
- **Zoom step**: `0.15` per click or scroll wheel (estimated)
- **Undo / Redo**: `Ctrl+Z` / `Ctrl+Y` (estimated)
- **Delete selected annotation**: `Delete` or `Backspace` (estimated)
- **Copy to clipboard**: `Ctrl+C` (estimated)
- **Export PNG**: `Ctrl+S` (estimated)

## Markup Tools
- **Tools**: Select, Arrow, Rectangle, Highlighter (35% alpha), Opaque Redaction, Text, Freehand, Crop (estimated)
- **Stroke widths**: Small `2.5 px`, Medium `4.0 px`, Large `7.0 px` (estimated)
- **Color palette**: 6 high-contrast Tailwind colors (Red, Amber, Emerald, Sky, Violet, White) (estimated)

## Album & Search
- **Storage**: Local PNG files + `album.json` metadata in `%LOCALAPPDATA%\Z-SHOT` (estimated)
- **Search**: Live filter across title, notes, timestamp, and text annotations (estimated)
- **Favorites**: Toggle star per capture and filter by favorites (estimated)
