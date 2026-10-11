[Setup]
AppId=BlueEngineGames.reverie
AppName=Reverie
AppVersion=0.1.0
AppPublisher=BlueEngine Games
DefaultDirName={localappdata}\BlueEngine\Games\reverie
DefaultGroupName=BlueEngine Games
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UsePreviousAppDir=yes
DisableProgramGroupPage=yes
OutputDir=..\installers
OutputBaseFilename=reverie-setup-windows-x64
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\reverie.exe
CloseApplications=yes
RestartApplications=no
[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked
[Files]
Source: "..\dist\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
[Icons]
Name: "{userprograms}\BlueEngine Games\Reverie"; Filename: "{app}\reverie.exe"; WorkingDir: "{app}"
Name: "{userdesktop}\Reverie"; Filename: "{app}\reverie.exe"; WorkingDir: "{app}"; Tasks: desktopicon
[Run]
Filename: "{app}\reverie.exe"; Description: "Enter Reverie"; Flags: nowait postinstall skipifsilent
