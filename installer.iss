[Setup]
AppName=IAMMUSIC
AppVersion=1.1.0
AppPublisher=IAMMUSIC
DefaultDirName={autopf}\IAMMUSIC
DefaultGroupName=IAMMUSIC
OutputDir=Output
OutputBaseFilename=IAMMUSIC_Installer
Compression=lzma2/ultra64
SolidCompression=yes
SetupIconFile=icon.ico
UninstallDisplayIcon={app}\IAMMUSIC.exe
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
WizardStyle=modern

; --- Professional Wizard Pages & Terms ---
DisableWelcomePage=no
DisableProgramGroupPage=no
DisableDirPage=no
LicenseFile=license.txt

; --- Windows Settings / App Removal Configuration ---
Uninstallable=yes
CreateUninstallRegKey=yes
CloseApplications=yes
CloseApplicationsFilter=IAMMUSIC.exe

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "startup"; Description: "Launch IAMMUSIC on Windows startup"; GroupDescription: "Startup:"

[Files]
Source: "dist\IAMMUSIC.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "icon.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "logo.png.png"; DestDir: "{app}"; Flags: ignoreversion
Source: "license.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\IAMMUSIC"; Filename: "{app}\IAMMUSIC.exe"; IconFilename: "{app}\icon.ico"
Name: "{group}\{cm:UninstallProgram,IAMMUSIC}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\IAMMUSIC"; Filename: "{app}\IAMMUSIC.exe"; IconFilename: "{app}\icon.ico"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "IAMMUSIC"; ValueData: """{app}\IAMMUSIC.exe"""; Flags: uninsdeletevalue; Tasks: startup
Root: HKCU; Subkey: "Software\IAMMUSIC"; Flags: uninsdeletekeyifempty

[InstallDelete]
; Clean up any previous version residue in the install directory
Type: files; Name: "{app}\IAMMUSIC_Update.exe"

[UninstallDelete]
; --- 100% Clean Removal (No Residue) ---
Type: files; Name: "{app}\*.*"
Type: files; Name: "{localappdata}\IAMMUSIC\*.*"
Type: files; Name: "{localappdata}\IAMMUSIC\*"
Type: dirifempty; Name: "{localappdata}\IAMMUSIC"
Type: files; Name: "{app}\*.log"
Type: files; Name: "{app}\*.tmp"
Type: dirifempty; Name: "{app}"

[Run]
Filename: "{app}\IAMMUSIC.exe"; Description: "{cm:LaunchProgram,IAMMUSIC}"; Flags: nowait postinstall skipifsilent
