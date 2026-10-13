# Names every process that holds a file, by asking the Restart Manager: the same question the
# setup program asks before it closes whatever holds the installed program. Prints one line per
# process, its id, its name and the command it was started with, or a line saying none holds it.
#   powershell -NoProfile -File file-holders.ps1 <file>
param([Parameter(Mandatory)][string]$File)
$ErrorActionPreference = 'Stop'
Add-Type -Namespace Holders -Name RestartManager -MemberDefinition @'
[StructLayout(LayoutKind.Sequential)]
public struct UniqueProcess { public int ProcessId; public System.Runtime.InteropServices.ComTypes.FILETIME StartTime; }
[StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
public struct ProcessInfo {
  public UniqueProcess Process;
  [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 256)] public string AppName;
  [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 64)] public string ServiceShortName;
  public int ApplicationType;
  public uint AppStatus;
  public uint TSSessionId;
  [MarshalAs(UnmanagedType.Bool)] public bool Restartable;
}
[DllImport("rstrtmgr.dll", CharSet = CharSet.Unicode)] public static extern int RmStartSession(out uint session, int flags, System.Text.StringBuilder key);
[DllImport("rstrtmgr.dll")] public static extern int RmEndSession(uint session);
[DllImport("rstrtmgr.dll", CharSet = CharSet.Unicode)] public static extern int RmRegisterResources(uint session, uint files, string[] names, uint processes, UniqueProcess[] none, uint services, string[] noServices);
[DllImport("rstrtmgr.dll")] public static extern int RmGetList(uint session, out uint needed, ref uint count, [In, Out] ProcessInfo[] found, ref uint reasons);
'@
$RM = [Holders.RestartManager]
$session = 0
$key = New-Object System.Text.StringBuilder 64
$failed = $RM::RmStartSession([ref]$session, 0, $key)
if ($failed) { throw "The Restart Manager would not start a session: error $failed." }
try {
  $failed = $RM::RmRegisterResources($session, 1, @($File), 0, $null, 0, $null)
  if ($failed) { throw "The Restart Manager would not take $File`: error $failed." }
  $needed = [uint32]0
  $count = [uint32]16
  $reasons = [uint32]0
  $found = New-Object 'Holders.RestartManager+ProcessInfo[]' 16
  $failed = $RM::RmGetList($session, [ref]$needed, [ref]$count, $found, [ref]$reasons)
  if ($failed) { throw "The Restart Manager could not list what holds $File`: error $failed." }
  if ($count -eq 0) {
    "Nothing holds $File."
    return
  }
  foreach ($holder in $found[0..($count - 1)]) {
    $id = $holder.Process.ProcessId
    $process = Get-CimInstance Win32_Process -Filter "ProcessId = $id"
    if ($process -and $process.CommandLine) {
      "Held by process $id, $($holder.AppName), started by $($process.ParentProcessId): $($process.CommandLine)"
    } else {
      "Held by process $id, $($holder.AppName), whose command this account cannot read."
    }
  }
} finally {
  [void]$RM::RmEndSession($session)
}
