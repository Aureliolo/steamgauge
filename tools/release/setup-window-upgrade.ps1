# Starts a setup program with its window and goes through it as a person would: prints the label
# of the welcome page's button and every text the page shows, presses the button, presses on
# through every page until the finish page, and prints each page's texts on the way. It then ends
# the setup program on the finish page, so nothing it would open at the end is opened.
#   powershell -NoProfile -File setup-window-upgrade.ps1 <setup.exe>
param([Parameter(Mandatory)][string]$Setup)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type -Namespace Setup -Name Window -MemberDefinition @'
[DllImport("user32.dll")] public static extern System.IntPtr GetDlgItem(System.IntPtr dialog, int id);
[DllImport("user32.dll")] public static extern bool IsWindowEnabled(System.IntPtr window);
[DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(System.IntPtr window, System.Text.StringBuilder text, int most);
[DllImport("user32.dll")] public static extern bool PostMessage(System.IntPtr window, uint message, System.IntPtr wParam, System.IntPtr lParam);
'@
$A = [System.Windows.Automation.AutomationElement]
$Descendants = [System.Windows.Automation.TreeScope]::Descendants
$Any = [System.Windows.Automation.Condition]::TrueCondition
$WM_COMMAND = 0x0111
# NSIS's own control ID for the button that moves to the next page, labelled Next, Install or
# Finish as the page needs.
$NEXT = 1

function Texts($window) {
  @($window.Current.Name) + @($window.FindAll($Descendants, $Any) | ForEach-Object { $_.Current.Name } | Where-Object { $_ })
}

function Label($button) {
  $text = New-Object System.Text.StringBuilder 256
  [void][Setup.Window]::GetWindowText($button, $text, $text.Capacity)
  $text.ToString().Replace('&', '')
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
  $dialog = $process.MainWindowHandle
  $window = $A::FromHandle($dialog)
  $button = [Setup.Window]::GetDlgItem($dialog, $NEXT)
  if ($button -eq [System.IntPtr]::Zero) { throw ("The setup program's window has no Next button. It shows:`n" + ((Texts $window) -join "`n")) }
  "Welcome button: $(Label $button)"
  Texts $window
  # UI Automation finds NSIS's plain Win32 buttons but offers no way to press them, so the press
  # is the message a button sends its dialog: WM_COMMAND with its ID and BN_CLICKED.
  $deadline = (Get-Date).AddSeconds(600)
  while ((Label $button) -ne 'Finish') {
    if ((Get-Date) -gt $deadline) { throw ("The setup program reached no finish page within ten minutes. It shows:`n" + ((Texts $window) -join "`n")) }
    $process.Refresh()
    if ($process.HasExited) { throw "The setup program ended before its finish page, with exit code $($process.ExitCode)." }
    if ([Setup.Window]::IsWindowEnabled($button)) {
      "--- Pressing $(Label $button)"
      [void][Setup.Window]::PostMessage($dialog, $WM_COMMAND, [System.IntPtr]$NEXT, $button)
      Start-Sleep -Seconds 2
      Texts $window
    } else {
      Start-Sleep -Seconds 1
    }
  }
  '--- Finish page'
  Texts $window
} finally {
  Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
}
