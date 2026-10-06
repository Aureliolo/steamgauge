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
  # NSIS's buttons are plain Win32 ones, found by their text and pressed through the pattern
  # every button offers.
  $invoke = $null
  foreach ($element in $window.FindAll($Descendants, $Any)) {
    if ($element.Current.Name -notlike 'Next*') { continue }
    $pattern = $null
    if ($element.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
      $invoke = $pattern
      break
    }
  }
  if (-not $invoke) { throw ("The first page has no Next button to press. It shows:`n" + ((Texts $window) -join "`n")) }
  $invoke.Invoke()
  Start-Sleep -Seconds 3
  Texts $window
} finally {
  Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
}
