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
; tauri.conf.json gives it, asking first as Tauri's check does, and moves the program and the
; DLLs beside it aside where something still holds them: Windows lets a file a running program was
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
    ; The rights to wait for the process to end and to end it.
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

; How long a held file is waited for, in rounds of STEAMGAUGE_PATIENCE_STEP milliseconds: a
; minute, for the setup program and the uninstaller alike.
!define STEAMGAUGE_PATIENCE_ROUNDS 240
!define STEAMGAUGE_PATIENCE_STEP 250

; The rounds an uninstall has waited so far. Everything it removes shares the one minute, so
; several held files cannot add up to several.
Var SteamGaugeWaited

; How many of SteamGauge's own files an uninstall could not remove.
Var SteamGaugeLeft

; Runs a removal until it leaves $R6 0, having removed all it is for, or the patience runs out.
; A process the uninstaller has just ended still holds its program and DLLs until Windows has
; torn it down, and a scanner may be reading a program just written, so a removal that fails is
; tried again. The uninstaller's log shows the first try alone.
!macro SteamGaugePatiently remove
  ${Do}
    !insertmacro ${remove}
    SetDetailsPrint none
    ${If} $R6 = 0
    ${OrIf} $SteamGaugeWaited >= ${STEAMGAUGE_PATIENCE_ROUNDS}
      ${Break}
    ${EndIf}
    IntOp $SteamGaugeWaited $SteamGaugeWaited + 1
    Sleep ${STEAMGAUGE_PATIENCE_STEP}
  ${Loop}
  SetDetailsPrint both
!macroend

; Deletes the file at $R5. A folder there, or a link to one, is not what the installer wrote,
; so it is left as it is: $R9 is then not 0. A link to a file goes as the link. The reparse
; point attribute does not tell a link here, since Windows sets it on a file it has compressed
; too.
!macro SteamGaugeRemoveFileOnce
  StrCpy $R6 0
  Push $R5
  System::Call 'kernel32::GetFileAttributesW(w s) i .R8'
  ${If} $R8 = -1
    StrCpy $R9 0
  ${Else}
    ; The folder attribute.
    IntOp $R9 $R8 & 0x10
    ${If} $R9 = 0
      Delete $R5
      ${IfThen} ${FileExists} $R5 ${|} StrCpy $R6 1 ${|}
    ${EndIf}
  ${EndIf}
!macroend

; Removes a file the installer wrote, patiently, and names it in the log where it stays.
; Windows refuses a file another program holds, a program still running and a file the person
; may not delete in much the same way, so the log does not say which.
!macro SteamGaugeRemoveFile path
  StrCpy $R5 "${path}"
  !insertmacro SteamGaugePatiently SteamGaugeRemoveFileOnce
  ${If} $R6 = 1
    DetailPrint "$R5 could not be removed."
    IntOp $SteamGaugeLeft $SteamGaugeLeft + 1
  ${ElseIf} $R9 <> 0
    DetailPrint "$R5 is not the file SteamGauge installed, so it stays."
  ${EndIf}
!macroend

; Removes all that is in the folder on the stack: a file as it is, a link as the link, and a
; real folder with all that is in it. NSIS's own RMDir /r enters a link to a folder and empties
; what it points at, which is someone else's.
!macro SteamGaugeEmptyFunction un
  Function ${un}SteamGaugeEmpty
    Exch $0
    Push $1
    Push $2
    Push $3
    FindFirst $1 $2 "$0\*"
    ${DoWhile} $2 != ""
      ${If} $2 != "."
      ${AndIf} $2 != ".."
        Push "$0\$2"
        System::Call 'kernel32::GetFileAttributesW(w s) i .r3'
        ; The folder and the reparse point attributes: a folder with both is a link to one.
        IntOp $3 $3 & 0x410
        ${If} $3 = 0x10
          Push "$0\$2"
          Call ${un}SteamGaugeEmpty
          RMDir "$0\$2"
        ${ElseIf} $3 = 0x410
          RMDir "$0\$2"
        ${Else}
          Delete "$0\$2"
        ${EndIf}
      ${EndIf}
      FindNext $1 $2
    ${Loop}
    FindClose $1
    Pop $3
    Pop $2
    Pop $1
    Pop $0
  FunctionEnd
!macroend
!insertmacro SteamGaugeEmptyFunction ""
!insertmacro SteamGaugeEmptyFunction "un."

; Names in the log every file left in the folder on the stack, and counts it.
Function un.SteamGaugeNameLeft
  Exch $0
  Push $1
  Push $2
  Push $3
  FindFirst $1 $2 "$0\*"
  ${DoWhile} $2 != ""
    ${If} $2 != "."
    ${AndIf} $2 != ".."
      Push "$0\$2"
      System::Call 'kernel32::GetFileAttributesW(w s) i .r3'
      IntOp $3 $3 & 0x410
      ${If} $3 = 0x10
        Push "$0\$2"
        Call un.SteamGaugeNameLeft
      ${Else}
        DetailPrint "$0\$2 could not be removed."
        IntOp $SteamGaugeLeft $SteamGaugeLeft + 1
      ${EndIf}
    ${EndIf}
    FindNext $1 $2
  ${Loop}
  FindClose $1
  Pop $3
  Pop $2
  Pop $1
  Pop $0
FunctionEnd

; Runs a macro for each folder an install moved files aside into, its name in $R5. Those are
; the real folders with a GUID's name in a real folder replaced. A link is never entered: what
; it points at is someone else's.
!macro SteamGaugeMovedAside each
  Push "$INSTDIR\replaced"
  System::Call 'kernel32::GetFileAttributesW(w s) i .R8'
  IntOp $R9 $R8 & 0x410
  ${If} $R9 = 0x10
    FindFirst $R4 $R5 "$INSTDIR\replaced\{*"
    ${DoWhile} $R5 != ""
      Push "$INSTDIR\replaced\$R5"
      System::Call 'kernel32::GetFileAttributesW(w s) i .R8'
      IntOp $R9 $R8 & 0x410
      ${If} $R9 = 0x10
        !insertmacro ${each}
      ${EndIf}
      FindNext $R4 $R5
    ${Loop}
    FindClose $R4
  ${EndIf}
!macroend

!macro SteamGaugeRemoveOneMovedAside
  Push "$INSTDIR\replaced\$R5"
  !ifdef __UNINSTALL__
    Call un.SteamGaugeEmpty
  !else
    Call SteamGaugeEmpty
  !endif
  RMDir "$INSTDIR\replaced\$R5"
  ${IfThen} ${FileExists} "$INSTDIR\replaced\$R5" ${|} StrCpy $R6 1 ${|}
!macroend

; Removes what installs moved aside, then replaced itself where nothing else is in it. Where
; replaced is a link to a folder, the link goes and the folder it points at is untouched.
!macro SteamGaugeRemoveMovedAsideOnce
  StrCpy $R6 0
  !insertmacro SteamGaugeMovedAside SteamGaugeRemoveOneMovedAside
  RMDir "$INSTDIR\replaced"
!macroend

; A folder moved aside that is still there holds something that would not go, so it counts
; whatever the walk finds to name in it.
!macro SteamGaugeNameOneMovedAside
  IntOp $SteamGaugeLeft $SteamGaugeLeft + 1
  Push "$INSTDIR\replaced\$R5"
  Call un.SteamGaugeNameLeft
!macroend

!macro SteamGaugeEndOneMovedAside
  ${If} ${FileExists} "$INSTDIR\replaced\$R5\${MAINBINARYNAME}.exe"
    !insertmacro CheckIfAppIsRunning "$INSTDIR\replaced\$R5\${MAINBINARYNAME}.exe" "${PRODUCTNAME}"
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
  !insertmacro SteamGaugeRemoveMovedAsideOnce
  System::Call 'ole32::CoCreateGuid(g .R7)'
  StrCpy $R9 0
  ${Do}
    !insertmacro SteamGaugeFree "${MAINBINARYNAME}.exe"
    StrCpy $R6 $R5
    FindFirst $R4 $R3 "$INSTDIR\*.dll"
    ${DoWhile} $R3 != ""
      !insertmacro SteamGaugeFree "$R3"
      ${IfThen} $R5 = 0 ${|} StrCpy $R6 0 ${|}
      FindNext $R4 $R3
    ${Loop}
    FindClose $R4
    ${If} $R6 = 1
      ${Break}
    ${EndIf}
    IntOp $R9 $R9 + 1
    ${If} $R9 >= ${STEAMGAUGE_PATIENCE_ROUNDS}
      FindFirst $R4 $R3 "$INSTDIR\replaced\$R7\*"
      ${DoWhile} $R3 != ""
        ${If} $R3 != "."
        ${AndIf} $R3 != ".."
          Rename "$INSTDIR\replaced\$R7\$R3" "$INSTDIR\$R3"
        ${EndIf}
        FindNext $R4 $R3
      ${Loop}
      FindClose $R4
      RMDir "$INSTDIR\replaced\$R7"
      RMDir "$INSTDIR\replaced"
      SetErrorLevel 5
      Abort "$INSTDIR\${MAINBINARYNAME}.exe or a DLL beside it is held open by another program and cannot be replaced."
    ${EndIf}
    Sleep ${STEAMGAUGE_PATIENCE_STEP}
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
; closes the installed one. That check stops the uninstall where the person refuses or the copy
; cannot be ended, so it runs before anything is removed: the entry in Apps is still there to
; uninstall from again.
!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro SteamGaugeMovedAside SteamGaugeEndOneMovedAside
!macroend

; By now the entry in Apps is gone, so a folder left here can never be uninstalled again, and
; only the person can finish: the log names each file of SteamGauge's that stayed and says what
; to do, and the uninstall ends with 5, the code the setup program gives for a held program.
; Windows takes a removal at the next restart from an administrator alone, and this uninstaller
; runs as the person. A setup program that uninstalls before it installs runs the uninstaller
; where it is (`_?=`), or with /UPDATE, and stops on any code but 0. It waits for and moves
; aside what is left itself, so for it the leftovers are logged and the code stays 0. A file of
; another program's in the folder is not SteamGauge's to remove or to report, so the folder
; stays with it and the uninstall succeeds.
!macro NSIS_HOOK_POSTUNINSTALL
  !insertmacro SteamGaugePatiently SteamGaugeRemoveMovedAsideOnce
  !insertmacro SteamGaugeMovedAside SteamGaugeNameOneMovedAside
  RMDir "$INSTDIR"
  ${If} $SteamGaugeLeft > 0
    DetailPrint "Close any program using them or check their permissions, then delete $INSTDIR."
    ${If} $UpdateMode <> 1
    ${AndIf} $EXEDIR != $INSTDIR
      SetErrorLevel 5
    ${EndIf}
  ${EndIf}
!macroend
