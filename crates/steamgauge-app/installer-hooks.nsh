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

; Tauri's setup program copies the program over the installed one, and when that copy fails in a
; silent install it carries on: the new libraries land beside the old program and the setup
; reports success. A running SteamGauge is closed first, as Tauri's own check would; a file still
; held after that, by a scanner reading a program just written or a process that has not yet let
; go, is waited out for up to a minute, and one held longer stops the install with an error
; rather than leaving two versions side by side.
!macro NSIS_HOOK_PREINSTALL
  !insertmacro CheckIfAppIsRunning "$INSTDIR\${MAINBINARYNAME}.exe" "${PRODUCTNAME}"
  ${If} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
    StrCpy $R9 0
    ${Do}
      ClearErrors
      FileOpen $R8 "$INSTDIR\${MAINBINARYNAME}.exe" a
      ${IfNot} ${Errors}
        FileClose $R8
        ${Break}
      ${EndIf}
      IntOp $R9 $R9 + 1
      ${If} $R9 >= 240
        SetErrorLevel 5
        Abort "$INSTDIR\${MAINBINARYNAME}.exe is held open by another program and cannot be replaced."
      ${EndIf}
      Sleep 250
    ${Loop}
  ${EndIf}
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
