param([string]$ExpectedDocument)
$ErrorActionPreference='Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$name=[IO.Path]::GetFileName($ExpectedDocument)
$found=$false
foreach($p in @(Get-Process -Name notepad -ErrorAction SilentlyContinue)){
 if($p.MainWindowHandle -eq 0){continue}
 $root=[System.Windows.Automation.AutomationElement]::FromHandle($p.MainWindowHandle)
 if($root.Current.Name -ne "$name - Notepad" -and $root.Current.Name -ne "*$name - Notepad"){continue}
 $document=$root.FindFirst([System.Windows.Automation.TreeScope]::Descendants,(New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty,[System.Windows.Automation.ControlType]::Document)))
 $document.SetFocus();$found=$true;break
}
if(-not $found){throw 'Exact test document is not open'}
