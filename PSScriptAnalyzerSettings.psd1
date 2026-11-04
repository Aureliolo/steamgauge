@{
  Severity = @('Error', 'Warning', 'Information')
  IncludeDefaultRules = $true
  Rules = @{
    PSAvoidExclaimOperator = @{ Enable = $true }
    PSAvoidLongLines = @{ Enable = $true; MaximumLineLength = 100 }
    PSAvoidSemicolonsAsLineTerminators = @{ Enable = $true }
    PSAvoidUsingDoubleQuotesForConstantString = @{ Enable = $true }
    PSPlaceCloseBrace = @{ Enable = $true; NoEmptyLineBefore = $true; NewLineAfter = $false }
    PSPlaceOpenBrace = @{ Enable = $true }
    # install-check.sh runs the scripts with Windows PowerShell, which every Windows ships.
    PSUseCompatibleCmdlets = @{ Compatibility = @('desktop-5.1.14393.206-windows') }
    PSUseCompatibleSyntax = @{ Enable = $true; TargetVersions = @('5.1') }
    PSUseConsistentIndentation = @{ Enable = $true; Kind = 'space'; IndentationSize = 2 }
    PSUseConsistentWhitespace = @{
      Enable = $true
      CheckPipe = $true
      CheckOperator = $true
      CheckSeparator = $true
      CheckInnerBrace = $true
      CheckOpenBrace = $true
      CheckOpenParen = $true
      CheckParameter = $true
    }
    PSUseCorrectCasing = @{ Enable = $true }
  }
}
