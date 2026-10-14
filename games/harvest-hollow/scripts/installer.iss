[Setup]
AppId=BlueEngineGames.harvest-hollow
AppName=Harvest Hollow
AppVersion=0.1.0
AppPublisher=BlueEngine Games
DefaultDirName={localappdata}\BlueEngine\Games\harvest-hollow
DefaultGroupName=BlueEngine Games
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UsePreviousAppDir=yes
DisableProgramGroupPage=yes
OutputDir=..\installers
OutputBaseFilename=harvest-hollow-setup-windows-x64
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\harvest-hollow.exe
CloseApplications=yes
RestartApplications=no

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked

[Files]
Source: "..\dist\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{userprograms}\BlueEngine Games\Harvest Hollow"; Filename: "{app}\harvest-hollow.exe"; WorkingDir: "{app}"
Name: "{userdesktop}\Harvest Hollow"; Filename: "{app}\harvest-hollow.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\harvest-hollow.exe"; Description: "Play Harvest Hollow"; Flags: nowait postinstall skipifsilent
