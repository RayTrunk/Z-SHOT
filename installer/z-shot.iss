; Z-SHOT — Copyright (C) 2026 R. Trunk
; SPDX-License-Identifier: GPL-3.0-or-later
;
; Inno Setup 7 script. Build with installer\build.ps1, which passes the version from
; app\Cargo.toml and the paths of the release exe and the app icon:
;   ISCC /DAppVersion=0.9.0 /DAppExe=..\app\target\release\z-shot.exe /DAppIcon=...\z-shot.ico z-shot.iss
;
; Per-user install (no admin rights): %LOCALAPPDATA%\Programs\Z-SHOT. Autostart uses the
; same registry value as the app's own setting (HKCU\...\Run, "Z-SHOT"), so the
; checkbox in Z-SHOT's settings shows what was chosen here.

#ifndef AppVersion
  #error "AppVersion fehlt - bitte installer\build.ps1 verwenden"
#endif
#ifndef AppExe
  #define AppExe "..\app\target\release\z-shot.exe"
#endif

#define AppName "Z-SHOT"
#define AppPublisher "R. Trunk"
#define AppExeName "z-shot.exe"

[Setup]
AppId={{CCC77DCE-F05C-4FF0-9BA7-145C27B16F4E}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
AppCopyright=Copyright (C) 2026 R. Trunk
VersionInfoVersion={#AppVersion}
VersionInfoDescription={#AppName} Setup
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; The GPL needs no acceptance to use the program: show it with a plain "Next".
InfoBeforeFile=..\LICENSE
OutputDir=Output
OutputBaseFilename=Z-SHOT-Setup-{#AppVersion}
#ifdef AppIcon
SetupIconFile={#AppIcon}
#endif
UninstallDisplayIcon={app}\{#AppExeName}
UninstallDisplayName={#AppName}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
; Language from the Windows display language; the dialog only appears if it is neither.
ShowLanguageDialog=auto
LanguageDetectionMethod=uilanguage
CloseApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "german"; MessagesFile: "compiler:Languages\German.isl"

[CustomMessages]
english.TaskAutostart=Start Z-SHOT automatically when Windows starts (runs in the background)
german.TaskAutostart=Z-SHOT beim Windows-Start automatisch starten (läuft im Hintergrund)
english.TaskGroupStart=Startup:
german.TaskGroupStart=Start:
english.LaunchApp=Start Z-SHOT now
german.LaunchApp=Z-SHOT jetzt starten
english.KeepDataNote=Your screenshots in the album are kept.
german.KeepDataNote=Ihre Screenshots im Album bleiben erhalten.

[Tasks]
Name: "autostart"; Description: "{cm:TaskAutostart}"; GroupDescription: "{cm:TaskGroupStart}"
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExeName}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; Tasks: desktopicon

[Registry]
; Written only when the task is chosen; on an update without it, the value is removed.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Z-SHOT"; ValueData: """{app}\{#AppExeName}"""; Tasks: autostart; Flags: uninsdeletevalue

[Run]
Filename: "{app}\{#AppExeName}"; Description: "{cm:LaunchApp}"; Flags: nowait postinstall skipifsilent

[Code]
// Z-SHOT keeps running in the tray, so close it before files are replaced or removed.
procedure StopZShot;
var
  Code: Integer;
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM {#AppExeName}', '', SW_HIDE,
    ewWaitUntilTerminated, Code);
  Sleep(300);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  StopZShot;
  Result := '';
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  // Autostart unticked on an update: remove a value left by an earlier install or by
  // the app's own setting.
  if (CurStep = ssPostInstall) and not WizardIsTaskSelected('autostart') then
    RegDeleteValue(HKEY_CURRENT_USER, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Z-SHOT');
end;

function InitializeUninstall: Boolean;
begin
  StopZShot;
  Result := True;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
  begin
    // The app may have set autostart itself; never leave a dead entry behind.
    RegDeleteValue(HKEY_CURRENT_USER, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Z-SHOT');
    if not UninstallSilent then
      MsgBox(CustomMessage('KeepDataNote'), mbInformation, MB_OK);
  end;
end;
