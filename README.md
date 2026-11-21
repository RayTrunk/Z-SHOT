# Z-SHOT

Screenshot-Werkzeug für Windows in Rust und egui. Es läuft im Hintergrund mit einem Symbol im Infobereich, nimmt per Tastenkürzel einen Bildschirmbereich auf und öffnet ihn danach im Editor oder speichert ihn direkt.

## Funktionen

- Bereichsauswahl per Tastenkürzel, Standard `Druck` und `Strg+Umschalt+S`, frei belegbar mit automatischer Tastenerkennung
- Nach der Auswahl: Editor öffnen, direkt speichern oder beides; optional zusätzlich in die Zwischenablage
- Speicherort `Bilder\Z-SHOT`, Dateiname nach Datum und Uhrzeit (`2026-11-21_14-05-33.png`)
- Editor mit Pfeil, Rechteck, Textmarker, Schwärzen, Text, Freihand und Zuschnitt; Undo/Redo
- Album mit Suche, Favoriten, Titeln und Notizen
- Autostart mit Windows (optional)

## Bauen

```
cd app
cargo build --release
```

Die Exe liegt danach unter `app/target/release/z-shot.exe`.

## Lizenz

Copyright (C) 2026 R. Trunk

Dieses Programm ist freie Software: Sie können es unter den Bedingungen der GNU General Public License, wie von der Free Software Foundation veröffentlicht, weitergeben und/oder modifizieren, entweder gemäß Version 3 der Lizenz oder (nach Ihrer Wahl) jeder späteren Version. Den vollständigen Lizenztext finden Sie in [LICENSE](LICENSE).
