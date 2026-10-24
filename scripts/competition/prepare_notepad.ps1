param([Parameter(Mandatory=$true)][string]$Workspace, [Parameter(Mandatory=$true)][string]$DocumentName)
$ErrorActionPreference='Stop'
$root=[System.IO.Path]::GetFullPath($Workspace)
$document=Join-Path $root $DocumentName
if(Test-Path -LiteralPath $document){throw 'Use a new isolated run directory'}
[System.IO.File]::WriteAllText($document,'',(New-Object System.Text.UTF8Encoding($false)))
$shortcut=(New-Object -ComObject WScript.Shell).CreateShortcut((Join-Path $root 'notepad.lnk'))
$shortcut.TargetPath=Join-Path $env:WINDIR 'System32\notepad.exe'
$shortcut.Arguments='"'+$document+'"'
$shortcut.WorkingDirectory=$root
$shortcut.Save()
