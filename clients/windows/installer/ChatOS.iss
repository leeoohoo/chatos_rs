#ifndef SourceDir
  #error SourceDir is required
#endif
#ifndef OutputDir
  #error OutputDir is required
#endif
#ifndef TargetPlatform
  #define TargetPlatform "x64"
#endif
#ifndef AppVersion
  #define AppVersion "3.0.0"
#endif

#define AppName "ChatOS"
#define AppPublisher "ChatOS"
#define AppExecutable "ChatOS.Desktop.exe"

[Setup]
AppId={{D9D7025E-C25B-4B9A-8C42-AF5E4301BBE8}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
DefaultDirName={localappdata}\Programs\ChatOS
DefaultGroupName=ChatOS
DisableProgramGroupPage=yes
OutputDir={#OutputDir}
OutputBaseFilename=ChatOS-Setup-{#TargetPlatform}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=lowest
CloseApplications=yes
RestartApplications=no
UninstallDisplayIcon={app}\{#AppExecutable}
SetupIconFile={#SourceDir}\Assets\ChatOS.ico
SetupLogging=yes
#if TargetPlatform == "ARM64"
ArchitecturesAllowed=arm64
ArchitecturesInstallIn64BitMode=arm64
#else
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
#endif

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[InstallDelete]
Type: files; Name: "{app}\Start-ChatOS.cmd"

[Icons]
Name: "{autoprograms}\ChatOS"; Filename: "{app}\{#AppExecutable}"; WorkingDir: "{app}"; IconFilename: "{app}\{#AppExecutable}"
Name: "{autodesktop}\ChatOS"; Filename: "{app}\{#AppExecutable}"; WorkingDir: "{app}"; IconFilename: "{app}\{#AppExecutable}"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional shortcuts:"; Flags: checkedonce

[Run]
Filename: "{app}\{#AppExecutable}"; Description: "Start ChatOS"; WorkingDir: "{app}"; Flags: postinstall skipifsilent nowait
