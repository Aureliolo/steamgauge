# Starts a setup program with its window, presses Next on the welcome page as a person would, and
# prints every text the page after it shows, one to a line; then ends the setup program without
# installing anything. The install check reads it to see which page an upgrade lands on.
#   powershell -NoProfile -File setup-next-page.ps1 <setup.exe>
param([Parameter(Mandatory)][string]$Setup)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$A = [System.Windows.Automation.AutomationElement]
$Descendants = [System.Windows.Automation.TreeScope]::Descendants
$Any = [System.Windows.Automation.Condition]::TrueCondition

function Texts($window) {
  @($window.Current.Name) + @($window.FindAll($Descendants, $Any) | ForEach-Object { $_.Current.Name } | Where-Object { $_ })
}

$process = Start-Process -FilePath $Setup -PassThru
try {
  $deadline = (Get-Date).AddSeconds(90)
  do {
    Start-Sleep -Milliseconds 500
    $process.Refresh()
  } until ($process.MainWindowHandle -ne 0 -or (Get-Date) -gt $deadline)
  if ($process.MainWindowHandle -eq 0) { throw 'The setup program showed no window within 90 seconds.' }
  Start-Sleep -Seconds 2
  $window = $A::FromHandle($process.MainWindowHandle)
  $next = $window.FindAll($Descendants, $Any) |
    Where-Object { $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button -and $_.Current.Name -like 'Next*' } |
    Select-Object -First 1
  if (-not $next) { throw ("The first page has no Next button. It shows:`n" + ((Texts $window) -join "`n")) }
  $next.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
  Start-Sleep -Seconds 3
  Texts $window
} finally {
  Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
}
