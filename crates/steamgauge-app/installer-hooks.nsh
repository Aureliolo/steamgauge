; The setup program keeps the folder a copy is installed in under Software\<publisher>\SteamGauge,
; and replacing a copy hands that folder to the copy's own uninstaller. Copies installed by
; SteamGauge 0.1.2 and earlier keep it under Software\Aurelio\SteamGauge, which this setup program
; does not read: replacing one from its window would hand the old uninstaller no folder, and fail.
; So the folder is carried over before the first page; a silent install shows no pages and
; runs no uninstaller. These lines are read before the setup program defines its own names, so the
; keys are spelled out.
!define MUI_CUSTOMFUNCTION_GUIINIT CarryOverInstallFolder
Function CarryOverInstallFolder
  ReadRegStr $0 SHCTX "Software\Aurelio Amoroso\SteamGauge" ""
  ${If} $0 == ""
    ReadRegStr $0 SHCTX "Software\Aurelio\SteamGauge" ""
    ${If} $0 != ""
      WriteRegStr SHCTX "Software\Aurelio Amoroso\SteamGauge" "" $0
      StrCpy $INSTDIR $0
    ${EndIf}
  ${EndIf}
FunctionEnd

; Every assistant SteamGauge is added to runs its own `steamgauge mcp` from the installed program
; for as long as the assistant runs, and one that is closed stays cut off until the person
; restarts the assistant. Tauri's own check closes every process the installed program runs in, so
; only the uninstaller asks it. Installing closes just the window, found by the class
; tauri.conf.json gives it, asking first as Tauri's check does, and moves the program and
; DirectML.dll aside where something still holds them: Windows lets a file a running program was
; loaded from be moved within its drive, though not written. Each install moves them into a
; folder of its own under replaced, and the next install or the uninstaller removes what nothing
; holds any more.
!macro SteamGaugeCloseTheWindow
  FindWindow $R0 "com.aureliolo.steamgauge"
  ${If} $R0 P<> 0
    ${IfNot} ${Silent}
    ${AndIf} $PassiveMode != 1
      nsis_tauri_utils::StrReplace "$(appRunningOkKill)" "{{product_name}}" "${PRODUCTNAME}"
      Pop $R1
      ${If} ${Cmd} `MessageBox MB_OKCANCEL $R1 IDCANCEL`
        nsis_tauri_utils::StrReplace "$(appRunning)" "{{product_name}}" "${PRODUCTNAME}"
        Pop $R1
        Abort $R1
      ${EndIf}
    ${EndIf}
    System::Call 'user32::GetWindowThreadProcessId(p R0, *i .R1)'
    ; SYNCHRONIZE and PROCESS_TERMINATE.
    System::Call 'kernel32::OpenProcess(i 0x00100001, i 0, i R1) p .R2'
    System::Call 'user32::PostMessageW(p R0, i ${WM_CLOSE}, p 0, p 0)'
    ${If} $R2 P<> 0
      ; A window still open after ten seconds is ended, as Tauri's check ends it.
      System::Call 'kernel32::WaitForSingleObject(p R2, i 10000) i .R3'
      ${If} $R3 <> 0
        System::Call 'kernel32::TerminateProcess(p R2, i 1)'
        System::Call 'kernel32::WaitForSingleObject(p R2, i 10000)'
      ${EndIf}
      System::Call 'kernel32::CloseHandle(p R2)'
    ${EndIf}
  ${EndIf}
!macroend

; Leaves $R5 1 where the file can be replaced: it is not there, it can be written, or it has been
; moved aside into replaced\$R7; 0 where it is held and cannot be moved either.
!macro SteamGaugeFree file
  StrCpy $R5 1
  ${If} ${FileExists} "$INSTDIR\${file}"
    ClearErrors
    FileOpen $R8 "$INSTDIR\${file}" a
    ${If} ${Errors}
      ClearErrors
      CreateDirectory "$INSTDIR\replaced\$R7"
      Rename "$INSTDIR\${file}" "$INSTDIR\replaced\$R7\${file}"
      ${IfThen} ${Errors} ${|} StrCpy $R5 0 ${|}
    ${Else}
      FileClose $R8
    ${EndIf}
  ${EndIf}
!macroend

; Tauri's setup program copies the program over the installed one, and when that copy fails in a
; silent install it carries on: the new libraries land beside the old program and the setup
; reports success. A file that can be neither written nor moved, held by a scanner reading a
; program just written or a process that has not yet let go, is waited out for up to a minute,
; and one held longer stops the install with an error, with whatever was moved aside put back,
; rather than leaving two versions side by side.
!macro NSIS_HOOK_PREINSTALL
  !insertmacro SteamGaugeCloseTheWindow
  RMDir /r "$INSTDIR\replaced"
  System::Call 'ole32::CoCreateGuid(g .R7)'
  StrCpy $R9 0
  ${Do}
    !insertmacro SteamGaugeFree "${MAINBINARYNAME}.exe"
    StrCpy $R6 $R5
    !insertmacro SteamGaugeFree "DirectML.dll"
    ${If} $R5 = 1
    ${AndIf} $R6 = 1
      ${Break}
    ${EndIf}
    IntOp $R9 $R9 + 1
    ${If} $R9 >= 240
      Rename "$INSTDIR\replaced\$R7\${MAINBINARYNAME}.exe" "$INSTDIR\${MAINBINARYNAME}.exe"
      Rename "$INSTDIR\replaced\$R7\DirectML.dll" "$INSTDIR\DirectML.dll"
      RMDir "$INSTDIR\replaced\$R7"
      RMDir "$INSTDIR\replaced"
      SetErrorLevel 5
      Abort "$INSTDIR\${MAINBINARYNAME}.exe or DirectML.dll is held open by another program and cannot be replaced."
    ${EndIf}
    Sleep 250
  ${Loop}
!macroend

; A quiet uninstall (`winget uninstall --silent`, a management tool) runs QuietUninstallString and
; falls back to UninstallString, which opens the uninstaller's window and waits on it. Tauri's
; setup program writes only the latter. The folder a 0.1.2 or earlier copy kept under
; Software\Aurelio is the one just written under the publisher's name, so that key goes.
!macro NSIS_HOOK_POSTINSTALL
  WriteRegStr SHCTX "${UNINSTKEY}" "QuietUninstallString" "$\"$INSTDIR\uninstall.exe$\" /S"
  DeleteRegKey SHCTX "Software\Aurelio\SteamGauge"
  DeleteRegKey /ifempty SHCTX "Software\Aurelio"
!macroend

; An assistant may still run a copy an install moved aside. Uninstalling closes it as Tauri's check
; closes the installed one, and then the folder can go.
!macro NSIS_HOOK_POSTUNINSTALL
  FindFirst $R4 $R5 "$INSTDIR\replaced\*"
  ${DoWhile} $R5 != ""
    ${If} ${FileExists} "$INSTDIR\replaced\$R5\${MAINBINARYNAME}.exe"
      !insertmacro CheckIfAppIsRunning "$INSTDIR\replaced\$R5\${MAINBINARYNAME}.exe" "${PRODUCTNAME}"
    ${EndIf}
    FindNext $R4 $R5
  ${Loop}
  FindClose $R4
  RMDir /r "$INSTDIR\replaced"
  RMDir "$INSTDIR"
!macroend
