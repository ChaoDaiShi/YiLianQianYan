param([Parameter(Mandatory=$true)][string]$OutputDirectory, [Parameter(Mandatory=$true)][string]$Phase, [Parameter(Mandatory=$true)][string]$ExpectedDocument, [switch]$Capture)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing
$windows = @()
$expectedName=[System.IO.Path]::GetFileName($ExpectedDocument)
foreach ($editor in @(Get-Process -Name notepad -ErrorAction SilentlyContinue)) {
    if ($editor.MainWindowHandle -eq 0) { continue }
    $root = [System.Windows.Automation.AutomationElement]::FromHandle($editor.MainWindowHandle)
    $title=$root.Current.Name
    $matches=($title -eq "$expectedName - Notepad" -or $title -eq "*$expectedName - Notepad" -or $title -eq "$expectedName - 记事本" -or $title -eq "*$expectedName - 记事本")
    if(-not $matches){
        $windows += @{pid=$editor.Id;document_matches_expected=$false;visible=(-not $root.Current.IsOffscreen);focused_document=$false;controls=@();screenshot=$null}
        continue
    }
    $bounds = $root.Current.BoundingRectangle
    $condition = New-Object System.Windows.Automation.OrCondition(
        (New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty,[System.Windows.Automation.ControlType]::Document)),
        (New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty,[System.Windows.Automation.ControlType]::Edit))
    )
    $controls = @()
    $captureBounds = $null
    foreach ($element in $root.FindAll([System.Windows.Automation.TreeScope]::Descendants,$condition)) {
        $pattern = $null
        $value = $null
        if ($element.TryGetCurrentPattern([System.Windows.Automation.TextPattern]::Pattern,[ref]$pattern)) {
            $value = $pattern.DocumentRange.GetText(8000)
        } elseif ($element.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern,[ref]$pattern)) {
            $value = $pattern.Current.Value
        }
        $controls += @{control_type=$element.Current.ControlType.ProgrammaticName;text=$value;focused=$element.Current.HasKeyboardFocus}
        if($element.Current.HasKeyboardFocus -and $element.Current.ControlType -eq [System.Windows.Automation.ControlType]::Document){$captureBounds=$element.Current.BoundingRectangle}
    }
    $screenshot = $null
    if ($Capture -and $captureBounds -and $captureBounds.Width -gt 0 -and $captureBounds.Height -gt 0 -and -not $root.Current.IsOffscreen) {
        $bitmap = New-Object System.Drawing.Bitmap([int]$captureBounds.Width,[int]$captureBounds.Height)
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.CopyFromScreen([int]$captureBounds.X,[int]$captureBounds.Y,0,0,$bitmap.Size)
            $screenshot = Join-Path $OutputDirectory ($Phase + '-notepad-' + $editor.Id + '.png')
            $bitmap.Save($screenshot,[System.Drawing.Imaging.ImageFormat]::Png)
        } finally { $graphics.Dispose(); $bitmap.Dispose() }
    }
    $windows += @{pid=$editor.Id;document_matches_expected=$true;title=$root.Current.Name;visible=(-not $root.Current.IsOffscreen);focused_document=(@($controls | Where-Object {$_.focused}).Count -gt 0);controls=$controls;screenshot=$screenshot}
}
$json = @{phase=$Phase;captured_at=(Get-Date).ToUniversalTime().ToString('o');windows=$windows} | ConvertTo-Json -Depth 8
[System.IO.File]::WriteAllText((Join-Path $OutputDirectory ($Phase + '-observation.json')),$json,(New-Object System.Text.UTF8Encoding($false)))
[Console]::WriteLine($json)
