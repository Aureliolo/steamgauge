# Starts a setup program with its window, presses Next on the welcome page as a person would, and
# prints every text the page after it shows, one to a line; then ends the setup program without
# installing anything. The install check reads it to see which page an upgrade lands on.
#   powershell -NoProfile -File setup-next-page.ps1 <setup.exe>
param([Parameter(Mandatory)][string]$Setup)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type -Namespace Setup -Name Window -MemberDefinition @'
[DllImport("user32.dll")] public static extern System.IntPtr GetParent(System.IntPtr window);
[DllImport("user32.dll")] public static extern int GetDlgCtrlID(System.IntPtr control);
[DllImport("user32.dll")] public static extern bool PostMessage(System.IntPtr window, uint message, System.IntPtr wParam, System.IntPtr lParam);
'@
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
  $next = [System.IntPtr]::Zero
  foreach ($element in $window.FindAll($Descendants, $Any)) {
    if ($element.Current.Name -like 'Next*' -and $element.Current.NativeWindowHandle -ne 0) {
      $next = [System.IntPtr]$element.Current.NativeWindowHandle
      break
    }
  }
  if ($next -eq [System.IntPtr]::Zero) { throw ("The first page has no Next button to press. It shows:`n" + ((Texts $window) -join "`n")) }
  # NSIS's buttons are plain Win32 ones that UI Automation finds but offers no way to press, so
  # the click is the message a button sends its dialog: WM_COMMAND with its ID and BN_CLICKED.
  $WM_COMMAND = 0x0111
  $id = [Setup.Window]::GetDlgCtrlID($next)
  if (-not [Setup.Window]::PostMessage([Setup.Window]::GetParent($next), $WM_COMMAND, [System.IntPtr]$id, $next)) {
    throw 'The Next button could not be pressed.'
  }
  Start-Sleep -Seconds 3
  Texts $window
} finally {
  Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
}
