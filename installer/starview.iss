; Inno Setup script for starview. Compiled in CI with:
;   ISCC.exe /DAppVersion=<version> installer\starview.iss

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{AB9FD514-1D10-41EF-AD47-DA92B96E53B9}}
AppName=starview
AppVersion={#AppVersion}
AppPublisher=pmaxhogan
AppPublisherURL=https://github.com/pmaxhogan/starview
AppSupportURL=https://github.com/pmaxhogan/starview/issues
DefaultDirName={userpf}\starview
DisableProgramGroupPage=yes
; Per-user install: no admin/UAC, silent self-updates work.
PrivilegesRequired=lowest
OutputBaseFilename=starview-setup
SetupIconFile=..\assets\starview.ico
UninstallDisplayIcon={app}\starview.exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Tasks]
Name: "startup"; Description: "Start starview when Windows starts"
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; Flags: unchecked

[Files]
Source: "..\target\release\starview.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{userprograms}\starview"; Filename: "{app}\starview.exe"
Name: "{userdesktop}\starview"; Filename: "{app}\starview.exe"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; \
  ValueType: string; ValueName: "starview"; ValueData: """{app}\starview.exe"""; \
  Tasks: startup; Flags: uninsdeletevalue

[Run]
; Interactive installs: offer to launch on the finish page.
Filename: "{app}\starview.exe"; Description: "Launch starview"; Flags: postinstall nowait skipifsilent
; Silent self-updates pass /RELAUNCH=1 to restart the app they just closed.
Filename: "{app}\starview.exe"; Flags: nowait; Check: IsSilentRelaunch

[Code]
function IsSilentRelaunch: Boolean;
begin
  Result := ExpandConstant('{param:RELAUNCH|0}') = '1';
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  ResultCode: Integer;
begin
  // The overlay has no close button; stop any running instance before copying.
  if CurStep = ssInstall then
    Exec('taskkill.exe', '/f /im starview.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;
