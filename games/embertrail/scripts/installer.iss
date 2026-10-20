[Setup]
AppId=BlueEngineGames.embertrail
AppName=Embertrail
AppVersion=0.2.4
AppPublisher=BlueEngine Games
DefaultDirName={localappdata}\BlueEngine\Games\embertrail
DefaultGroupName=BlueEngine Games
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UsePreviousAppDir=yes
DisableProgramGroupPage=yes
OutputDir=..\installers
OutputBaseFilename=embertrail-setup-windows-x64
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\embertrail.exe
CloseApplications=yes
RestartApplications=no
[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked
[Files]
Source: "..\dist\embertrail.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\dist\embertrail.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\dist\embertrail.png"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\dist\assets\audio\*"; DestDir: "{app}\assets\audio"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\dist\assets\fonts\*"; DestDir: "{app}\assets\fonts"; Flags: ignoreversion recursesubdirs createallsubdirs
[Icons]
Name: "{userprograms}\BlueEngine Games\Embertrail"; Filename: "{app}\embertrail.exe"; WorkingDir: "{app}"
Name: "{userdesktop}\Embertrail"; Filename: "{app}\embertrail.exe"; WorkingDir: "{app}"; Tasks: desktopicon
[Run]
Filename: "{app}\embertrail.exe"; Description: "Play Embertrail"; Flags: nowait postinstall skipifsilent
