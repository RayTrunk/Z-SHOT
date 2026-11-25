# Flatpak

Stand: vorbereitet, aber noch nicht gebaut oder getestet.

## Bauen

Voraussetzung: `flatpak`, `flatpak-builder` und das Flathub-Repository.

```
flatpak install flathub org.freedesktop.Platform//25.08 org.freedesktop.Sdk//25.08 \
    org.freedesktop.Sdk.Extension.rust-stable//25.08

# Cargo-Quellen für den Offline-Build erzeugen (nach jeder Änderung an app/Cargo.lock)
python3 flatpak-cargo-generator.py ../../app/Cargo.lock -o cargo-sources.json

flatpak-builder --user --install --force-clean build io.github.raytrunk.ZShot.yml
flatpak run io.github.raytrunk.ZShot
```

`flatpak-cargo-generator.py` stammt aus
[flatpak/flatpak-builder-tools](https://github.com/flatpak/flatpak-builder-tools/tree/master/cargo).

## Rechte der Sandbox

| Recht | Wofür |
|---|---|
| `--socket=wayland`, `--socket=fallback-x11`, `--device=dri` | Fenster und OpenGL |
| `--share=network` | Update-Prüfung bei `api.github.com` |
| `--talk-name=org.freedesktop.Notifications` | Benachrichtigungen |
| `--talk-name=org.kde.StatusNotifierWatcher` | Symbol im Infobereich |
| `--filesystem=xdg-pictures` | Standard-Speicherort `Bilder/Z-SHOT` |

Bildschirmaufnahme, globale Tastenkürzel, Dateidialoge und Autostart laufen über die
Desktop-Portale und brauchen keine eigenen Rechte.
