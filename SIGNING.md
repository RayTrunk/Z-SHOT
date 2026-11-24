# Code-Signierung mit SignPath

Z-SHOT wird über die [SignPath Foundation](https://signpath.org) kostenlos signiert (Angebot für Open-Source-Projekte). Gebaut und signiert wird ausschließlich in GitHub Actions ([`.github/workflows/release.yml`](.github/workflows/release.yml)); lokal gebaute Dateien werden nie signiert.

## Ablauf eines Releases (sobald alles eingerichtet ist)

1. Version in `app/Cargo.toml` erhöhen, committen, pushen.
2. Tag setzen und pushen: `git tag v0.9.2 && git push origin v0.9.2`.
3. Der Workflow baut und testet, schickt `z-shot.exe` zu SignPath (**Anfrage 1**), baut damit die Setup-Datei und schickt sie zu SignPath (**Anfrage 2**).
4. Jede Anfrage muss auf [app.signpath.io](https://app.signpath.io) von Hand genehmigt werden („Approve“). Der Workflow wartet darauf bis zu 6 Stunden.
5. Am Ende liegt ein **Entwurf** des Releases mit der signierten Setup-Datei auf GitHub. Änderungen eintragen und veröffentlichen. Erst dann sehen ihn der Update-Checker und die Nutzer.

Ohne SignPath-Zugangsdaten läuft der Workflow genauso, nur ohne Signatur.

## Einmalige Einrichtung

### 1. Voraussetzungen

- Zwei-Faktor-Anmeldung (MFA) für das GitHub-Konto einschalten, später ebenso für SignPath. SignPath verlangt das für alle Teammitglieder.
- Das Repo ist öffentlich, unter GPL-3.0, und hat bereits ein Release (v0.9.1). Das ist erfüllt.
- In der README stehen „Code signing policy“, Teamrollen und Datenschutzerklärung. Das ist erfüllt.

### 2. Bewerben

Auf <https://signpath.org/apply> das Formular ausfüllen:

- Projekt: Z-SHOT, Repository `https://github.com/RayTrunk/Z-SHOT`
- Download-Seite: `https://github.com/RayTrunk/Z-SHOT/releases`
- Lizenz: GPL-3.0-or-later
- Build-System: GitHub Actions (GitHub-hosted runners)
- Zu signieren: `z-shot.exe` und `Z-SHOT-Setup-<version>.exe` (Inno Setup)

SignPath prüft den Antrag von Hand und meldet sich per E-Mail. Eine Garantie auf Annahme gibt es nicht.

### 3. SignPath-Projekt einrichten (nach der Zusage)

Auf app.signpath.io:

1. **Trusted Build System:** In der Organisation „GitHub.com“ hinzufügen und mit dem Projekt verknüpfen. Optional die [SignPath GitHub App](https://github.com/apps/signpath) für das Repo installieren.
2. **Projekt** mit dem Slug `Z-SHOT` anlegen, falls SignPath es nicht schon angelegt hat.
3. **Artifact Configurations** anlegen und den Inhalt der Dateien hier im Repo einfügen:
   - Slug `app`: [`.signpath/artifact-configurations/app.xml`](.signpath/artifact-configurations/app.xml)
   - Slug `installer`: [`.signpath/artifact-configurations/installer.xml`](.signpath/artifact-configurations/installer.xml)
4. **Signing Policy** mit dem Slug `release-signing` (Zertifikat der SignPath Foundation, manuelle Genehmigung). Sich selbst als Submitter und Approver eintragen.
5. **API-Token:** Für einen Benutzer mit Submitter-Recht einen Token erzeugen (oder einen CI-Benutzer anlegen).

Wenn SignPath andere Slugs vorgibt, trägt man sie als GitHub-Variablen ein (siehe Schritt 4); die Werte oben sind nur die Vorgaben des Workflows.

### 4. GitHub einrichten

Im Repo unter *Settings → Secrets and variables → Actions*:

| Art | Name | Wert |
|---|---|---|
| Secret | `SIGNPATH_API_TOKEN` | API-Token aus Schritt 3.5 |
| Variable | `SIGNPATH_ORGANIZATION_ID` | Organization ID (SignPath → Organization → Settings) |
| Variable (optional) | `SIGNPATH_PROJECT_SLUG` | nur falls nicht `Z-SHOT` |
| Variable (optional) | `SIGNPATH_POLICY_SLUG` | nur falls nicht `release-signing` |

Per Kommandozeile:

```
gh secret set SIGNPATH_API_TOKEN --repo RayTrunk/Z-SHOT
gh variable set SIGNPATH_ORGANIZATION_ID --repo RayTrunk/Z-SHOT --body "<id>"
```

### 5. Testlauf

Unter *Actions → Release → Run workflow* einen Lauf ohne Tag starten. Er baut, lässt signieren (zwei Genehmigungen auf SignPath) und prüft am Ende die Signaturen. Die Setup-Datei liegt danach als Artefakt am Lauf.

## Bekannte offene Punkte

- **Inno Setup füllt die Versionsinfos mit Leerzeichen auf.** In der Setup-Datei steht `ProductName` als „Z-SHOT“ mit angehängten Leerzeichen, ebenso `ProductVersion`. Die SignPath-Dokumentation sagt nicht, ob beim Vergleich Leerzeichen am Ende ignoriert werden. Falls Anfrage 2 deshalb abgelehnt wird: SignPath-Support fragen oder die Einschränkungen in `installer.xml` anpassen.
- **Signaturen gelten ab der ersten Version nach der Zusage.** Ältere Releases (bis 0.9.1) bleiben unsigniert.
