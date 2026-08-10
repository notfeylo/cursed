<#
.SYNOPSIS
  Proves that uninstalling Cursed left nothing behind.

.DESCRIPTION
  Two modes, and the order matters:

      powershell -File scripts/verify-uninstall.ps1 -Snapshot   # BEFORE installing
      powershell -File scripts/verify-uninstall.ps1             # AFTER uninstalling

  The snapshot records the seventeen HKCU cursor values as they were on a
  machine that has never run Cursed. Without it the check after uninstall can
  only assert that nothing named Cursed remains, which is the easy half — the
  half that matters is that every pointer role came back to the exact value it
  had before, because a restore that writes a *plausible* scheme rather than the
  original one looks completely fine and is still wrong.

  Everything here is HKCU and per-user. Nothing needs administrator rights,
  which is the same promise the installer makes.

.NOTES
  Exit code 0 means clean. Non-zero is the number of failed assertions, so this
  can gate a release from CI or a VM run.
#>
[CmdletBinding()]
param(
  # Write the pre-install baseline instead of checking against it.
  [switch]$Snapshot,

  # Where the baseline lives. Deliberately outside the repo by default: it
  # describes one machine and committing it would invite comparing machine A
  # against machine B, which fails for reasons that have nothing to do with the
  # uninstaller.
  [string]$BaselinePath = (Join-Path $env:TEMP 'cursed-uninstall-baseline.json')
)

$ErrorActionPreference = 'Stop'

# The bundle identifier, not the product name. WebView2 keys its user-data
# folder on the identifier, so this is the path that actually holds the ~70 MB
# EBWebView cache — looking for %LOCALAPPDATA%\Cursed finds nothing and reports
# a clean machine.
$Identifier = 'dev.feylo.cursed'

$CursorKey  = 'HKCU:\Control Panel\Cursors'
$SchemesKey = 'HKCU:\Control Panel\Cursors\Schemes'
$RunKey     = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'

# The seventeen roles Cursed writes, by their registry value names.
$Roles = @(
  'Arrow','Help','AppStarting','Wait','Crosshair','IBeam','NWPen','No',
  'SizeNS','SizeWE','SizeNWSE','SizeNESW','SizeAll','UpArrow','Hand','Pin','Person'
)

function Get-CursorValues {
  # Read through the .NET registry API with DoNotExpandEnvironmentNames, NOT
  # Get-ItemProperty.
  #
  # These values are REG_EXPAND_SZ and Windows stores them unexpanded --
  # `%SystemRoot%\cursors\aero_arrow.cur`. Get-ItemProperty expands them on the
  # way out, so comparing its output against the snapshot reports all seventeen
  # roles as changed on a machine where the restore was byte-perfect. That is
  # exactly the false alarm that sends someone off to "fix" correct code.
  $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Control Panel\Cursors')
  $out = [ordered]@{}
  try {
    foreach ($role in $Roles) {
      $value = if ($key) {
        $key.GetValue($role, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
      } else { $null }
      # An absent value and an empty value both mean "Windows draws its
      # default", so they are recorded identically. Treating them as different
      # fails on a machine that was simply never customised.
      $out[$role] = if ([string]::IsNullOrEmpty($value)) { '' } else { [string]$value }
    }
  } finally {
    if ($key) { $key.Close() }
  }
  $out
}

if ($Snapshot) {
  $baseline = [ordered]@{
    takenAt = (Get-Date).ToString('o')
    machine = $env:COMPUTERNAME
    cursors = Get-CursorValues
  }
  $baseline | ConvertTo-Json -Depth 5 | Set-Content -Path $BaselinePath -Encoding utf8
  Write-Host "Baseline written to $BaselinePath" -ForegroundColor Green
  Write-Host "Install, use, then uninstall Cursed, and run this script again with no arguments."
  exit 0
}

$failures = 0
function Assert-Clean {
  param([string]$What, [bool]$Ok, [string]$Detail = '')
  if ($Ok) {
    Write-Host ("  ok    {0}" -f $What) -ForegroundColor DarkGray
  } else {
    Write-Host ("  FAIL  {0}{1}" -f $What, $(if ($Detail) { " -- $Detail" } else { '' })) -ForegroundColor Red
    $script:failures++
  }
}

Write-Host "`nFiles" -ForegroundColor Cyan
Assert-Clean 'no %APPDATA%\Cursed'                  (-not (Test-Path "$env:APPDATA\Cursed"))
Assert-Clean 'no %LOCALAPPDATA%\Cursed'             (-not (Test-Path "$env:LOCALAPPDATA\Cursed"))
Assert-Clean "no %LOCALAPPDATA%\$Identifier (WebView2)" (-not (Test-Path "$env:LOCALAPPDATA\$Identifier"))

Write-Host "`nRegistry" -ForegroundColor Cyan
$run = (Get-ItemProperty $RunKey -ErrorAction SilentlyContinue)
Assert-Clean 'no autostart Run entry' ($null -eq $run.Cursed) $run.Cursed

$schemes = (Get-Item $SchemesKey -ErrorAction SilentlyContinue).Property
$ours = @($schemes | Where-Object { $_ -like 'Cursed*' -or $_ -like 'CursorForge*' })
Assert-Clean 'no Cursed cursor schemes' ($ours.Count -eq 0) ($ours -join ', ')

foreach ($hive in @('HKCU:\Software\Cursed', "HKCU:\Software\$Identifier")) {
  Assert-Clean "no $hive" (-not (Test-Path $hive))
}

$uninstall = Get-ItemProperty 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*' -ErrorAction SilentlyContinue |
  Where-Object { $_.DisplayName -like '*Cursed*' }
Assert-Clean 'no uninstall entry' ($null -eq $uninstall) $uninstall.DisplayName

Write-Host "`nShortcuts" -ForegroundColor Cyan
$shortcuts = @(
  "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Cursed",
  "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Cursed.lnk",
  "$env:USERPROFILE\Desktop\Cursed.lnk"
)
foreach ($path in $shortcuts) {
  Assert-Clean "no $(Split-Path $path -Leaf)" (-not (Test-Path $path))
}

Write-Host "`nPointer roles restored" -ForegroundColor Cyan
if (-not (Test-Path $BaselinePath)) {
  Write-Host "  SKIP  no baseline at $BaselinePath -- re-run with -Snapshot before the next install." -ForegroundColor Yellow
  Write-Host "        Checking only that no role points at a file that no longer exists." -ForegroundColor Yellow
  $now = Get-CursorValues
  foreach ($role in $Roles) {
    $value = $now[$role]
    if ([string]::IsNullOrEmpty($value)) { continue }
    $expanded = [Environment]::ExpandEnvironmentVariables($value)
    # This is the failure that leaves someone with an invisible pointer and no
    # obvious way back: the registry still names a .cur the uninstaller deleted.
    Assert-Clean "$role points at a file that exists" (Test-Path $expanded) $expanded
  }
} else {
  $baseline = Get-Content $BaselinePath -Raw | ConvertFrom-Json
  $now = Get-CursorValues
  foreach ($role in $Roles) {
    $was = [string]$baseline.cursors.$role
    $is  = [string]$now[$role]
    Assert-Clean "$role matches the pre-install value" ($was -eq $is) "was '$was', now '$is'"
  }
}

Write-Host ''
if ($failures -eq 0) {
  Write-Host 'Clean. Uninstall left no residue.' -ForegroundColor Green
} else {
  Write-Host "$failures check(s) failed." -ForegroundColor Red
}
exit $failures
