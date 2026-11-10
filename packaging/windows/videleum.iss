#define MyAppName "Videleum"
#define MyAppPublisher "Videleum"
#define MyAppExeName "videleum.exe"

#ifndef MyAppVersion
    #define MyAppVersion "0.1.0"
#endif

#ifndef RepoRoot
    #define RepoRoot "..\.."
#endif

#ifndef OutputDir
    #define OutputDir "..\..\dist"
#endif

#ifndef SourceExe
    #define SourceExe "videleum.exe"
#endif

[Setup]
AppId={{4C8E3B8A-5D9A-4A83-9F5A-1E2E5B7C6D31}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={autopf}\Videleum
DefaultGroupName=Videleum
DisableProgramGroupPage=yes
AllowNoIcons=yes
OutputDir={#OutputDir}
OutputBaseFilename=Videleum-{#MyAppVersion}-windows-x64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64
ArchitecturesInstallIn64BitMode=x64
PrivilegesRequired=lowest
UninstallDisplayIcon={app}\{#MyAppExeName}
LicenseFile={#RepoRoot}\LICENSE

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional icons:"

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#RepoRoot}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#RepoRoot}\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Videleum"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\Videleum"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "Launch Videleum"; Flags: nowait postinstall skipifsilent
