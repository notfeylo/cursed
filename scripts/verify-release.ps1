<#
.SYNOPSIS
  Runs the whole release verification matrix on a clean VM and prints a
  pass/fail table.

.DESCRIPTION
  One command, one table, paste it into docs/verification/. It covers three
  things that cannot be checked on the machine that builds them:

    - first install: what an installer leaves on a machine that has never seen
      this app;
    - the update path: that N -> N+1 keeps every byte of the user's own data,
      needs one click, shows no uninstall step, and brings the app back;
    - uninstall: that nothing survives it, delegated to verify-uninstall.ps1
      which already does that job properly.

  It is *guided*, not fully automatic, and that is deliberate. An in-app update
  is started by a person clicking a button in a window, and a script that drove
  that button would be verifying a path no user takes. So the script does
  everything that can be done without a human, and stops with a clear
  instruction where a human is required.

  State is kept in a file between steps, so a run survives the reboot an
  installer might ask for. Re-running picks up where it stopped.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\verify-release.ps1 `
    -From C:\builds\Cursed_1.20.0_x64-setup.exe -To 1.21.0

.NOTES
  Exit code is the number of failed rows, so this can gate a release.
  Everything here is per-user. Nothing needs administrator rights.
#>
[CmdletBinding()]
param(
  # The OLDER installer -- the version the update is from.
  [Parameter(Mandatory = $true)]
  [string]$From,

  # The version the update should produce, e.g. 1.21.0.
  [Parameter(Mandatory = $true)]
  [string]$To,

  # Start over rather than resuming.
  [switch]$Restart,

  # Run against a machine that already holds real data. Refused by default.
  [switch]$Force,

  [string]$StatePath = (Join-Path $env:TEMP 'cursed-verify-release.json')
)

$ErrorActionPreference = 'Stop'

$Identifier = 'dev.feylo.cursed'
$DataDir    = Join-Path $env:APPDATA 'Cursed'
$Roles = @(
  'Arrow','Help','AppStarting','Wait','Crosshair','IBeam','NWPen','No',
  'SizeNS','SizeWE','SizeNWSE','SizeNESW','SizeAll','UpArrow','Hand','Pin','Person'
)

# --- the table ---------------------------------------------------

$script:Rows = New-Object System.Collections.ArrayList

function Add-Row {
  param(
    [string]$Section,
    [string]$What,
    # 'pass', 'fail', 'skip'. A skip is never a pass; it prints as its own thing
    # and is counted separately, because an unrunnable check recorded as a
    # passing one is how a gap becomes permanent.
    [string]$Result,
    [string]$Detail = ''
  )
  [void]$script:Rows.Add([pscustomobject]@{
    Section = $Section
    What    = $What
    Result  = $Result
    Detail  = $Detail
  })
  $colour = switch ($Result) { 'pass' { 'DarkGray' } 'fail' { 'Red' } default { 'Yellow' } }
  $label  = switch ($Result) { 'pass' { 'ok  ' } 'fail' { 'FAIL' } default { 'skip' } }
  Write-Host ("  {0}  {1}{2}" -f $label, $What, $(if ($Detail) { " -- $Detail" } else { '' })) -ForegroundColor $colour
}

function Assert-Row {
  param([string]$Section, [string]$What, [bool]$Ok, [string]$Detail = '')
  Add-Row -Section $Section -What $What -Result $(if ($Ok) { 'pass' } else { 'fail' }) -Detail $Detail
}

function Section {
  param([string]$Title)
  Write-Host "`n$Title" -ForegroundColor Cyan
}

function Pause-For {
  param([string]$Instruction)
  Write-Host ''
  Write-Host '  ACTION NEEDED' -ForegroundColor Magenta
  foreach ($line in $Instruction -split "`n") { Write-Host "    $line" -ForegroundColor Magenta }
  Write-Host ''
  Read-Host '  Press Enter when that is done'
}

# --- reading the machine -----------------------------------------

function Get-CursorValues {
  # DoNotExpandEnvironmentNames, for the reason verify-uninstall.ps1 spells out:
  # these are REG_EXPAND_SZ, and a reader that expands them reports all
  # seventeen roles as changed on a machine where nothing changed.
  $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Control Panel\Cursors')
  $out = [ordered]@{}
  try {
    foreach ($role in $Roles) {
      $value = if ($key) {
        $key.GetValue($role, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
      } else { $null }
      $out[$role] = if ([string]::IsNullOrEmpty($value)) { '' } else { [string]$value }
    }
  } finally {
    if ($key) { $key.Close() }
  }
  $out
}

function Get-InstalledExe {
  # Where the per-user NSIS build puts itself, plus the uninstall entry as a
  # fallback for a layout that changes.
  $candidates = @(
    (Join-Path $env:LOCALAPPDATA 'Cursed\Cursed.exe'),
    (Join-Path ${env:ProgramFiles} 'Cursed\Cursed.exe')
  )
  foreach ($path in $candidates) { if (Test-Path $path) { return $path } }

  $entry = Get-ItemProperty 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*' -ErrorAction SilentlyContinue |
    Where-Object { $_.DisplayName -like '*Cursed*' } | Select-Object -First 1
  if ($entry -and $entry.InstallLocation) {
    $path = Join-Path $entry.InstallLocation 'Cursed.exe'
    if (Test-Path $path) { return $path }
  }
  $null
}

function Get-InstalledVersion {
  $entry = Get-ItemProperty 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*' -ErrorAction SilentlyContinue |
    Where-Object { $_.DisplayName -like '*Cursed*' } | Select-Object -First 1
  if ($entry) { return [string]$entry.DisplayVersion }
  $exe = Get-InstalledExe
  if ($exe) { return (Get-Item $exe).VersionInfo.ProductVersion }
  ''
}

function Get-DataPrint {
  param([string]$Dest)
  $exe = Get-InstalledExe
  if (-not $exe) { return $false }
  # The app fingerprints its own data directory: it is the only thing that knows
  # which parts are the user's and which are rendered cache. See
  # src-tauri/src/dataprint.rs.
  & $exe '--data-print' $Dest | Out-Null
  return (Test-Path $Dest)
}

function Load-State {
  if ($Restart -or -not (Test-Path $StatePath)) {
    return [ordered]@{ step = 'preflight'; startedAt = (Get-Date).ToString('o') }
  }
  Get-Content $StatePath -Raw | ConvertFrom-Json
}

function Save-State {
  param($State)
  $State | ConvertTo-Json -Depth 6 | Set-Content -Path $StatePath -Encoding utf8
}

$state = Load-State
$beforePrint = Join-Path $env:TEMP 'cursed-data-before.json'
$afterPrint  = Join-Path $env:TEMP 'cursed-data-after.json'
$baselinePath = Join-Path $env:TEMP 'cursed-uninstall-baseline.json'

Write-Host "Cursed release verification" -ForegroundColor White
Write-Host "  from     $From"
Write-Host "  to       $To"
Write-Host "  machine  $env:COMPUTERNAME, $([Environment]::OSVersion.Version)"

# --- 0. preflight ------------------------------------------------
#
# The most likely way to lose the author's own data is to run this on the wrong
# machine. So the first thing it does is refuse to.

Section '0. Preflight'

if (Test-Path $DataDir) {
  $size = (Get-ChildItem $DataDir -Recurse -File -ErrorAction SilentlyContinue |
    Measure-Object -Property Length -Sum).Sum
  $mb = [math]::Round(($size / 1MB), 1)
  if ($size -gt 20MB -and -not $Force) {
    Write-Host ''
    Write-Host "  REFUSING TO RUN" -ForegroundColor Red
    Write-Host "  $DataDir holds $mb MB. This looks like a real install, and this" -ForegroundColor Red
    Write-Host "  script verifies a failure mode that deletes that directory." -ForegroundColor Red
    Write-Host "  Roll back to the pristine VM snapshot, or pass -Force if you are" -ForegroundColor Red
    Write-Host "  certain this data is disposable." -ForegroundColor Red
    exit 1
  }
  Add-Row '0' "existing data directory ($mb MB)" 'skip' 'not a first-install machine'
} else {
  Add-Row '0' 'no existing data directory' 'pass' 'this is a first-install machine'
}

if (-not (Test-Path $From)) {
  Write-Host "  FAIL  -From does not exist: $From" -ForegroundColor Red
  exit 1
}

# --- 1. baseline -------------------------------------------------

Section '1. Baseline, before anything is installed'

if (-not $state.baseline) {
  $baseline = [ordered]@{
    takenAt = (Get-Date).ToString('o')
    machine = $env:COMPUTERNAME
    cursors = Get-CursorValues
  }
  $baseline | ConvertTo-Json -Depth 5 | Set-Content -Path $baselinePath -Encoding utf8
  $state | Add-Member -NotePropertyName baseline -NotePropertyValue $baselinePath -Force
  Save-State $state
}
Assert-Row '1' 'the seventeen pre-install pointer values recorded' (Test-Path $baselinePath) $baselinePath

$installedBefore = Get-InstalledExe
if ($installedBefore) {
  Add-Row '1' 'nothing installed yet' 'skip' "already installed at $installedBefore"
} else {
  Add-Row '1' 'nothing installed yet' 'pass'
}

# --- 2. first install --------------------------------------------

Section "2. First install ($(Split-Path $From -Leaf))"

if (-not (Get-InstalledExe)) {
  Write-Host '  running the installer silently...' -ForegroundColor DarkGray
  # /S is right here and wrong for an update: this step is setting up the
  # machine, not exercising the path a user takes.
  Start-Process -FilePath $From -ArgumentList '/S' -Wait
  Start-Sleep -Seconds 3
}

$exe = Get-InstalledExe
Assert-Row '2' 'the app is installed' ($null -ne $exe) $exe
Assert-Row '2' 'an uninstall entry exists' ('' -ne (Get-InstalledVersion)) (Get-InstalledVersion)
Assert-Row '2' 'a Start menu shortcut exists' `
  (Test-Path "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Cursed.lnk")

if (-not $exe) {
  Write-Host "`nThe installer produced nothing runnable. Stopping." -ForegroundColor Red
  exit 1
}

# --- 3. seed it with something worth losing ----------------------

Section '3. Data to lose'

if (-not $state.seeded) {
  Pause-For @"
Launch Cursed, and give it something the update could destroy:
  1. apply a cursor from the catalog
  2. import an image and build a custom cursor from it
  3. save a preset
Then leave the app running, with the window open.
"@
  $state | Add-Member -NotePropertyName seeded -NotePropertyValue $true -Force
  Save-State $state
}

Assert-Row '3' 'the data directory exists' (Test-Path $DataDir) $DataDir
Assert-Row '3' 'the original-scheme snapshot was captured' `
  (Test-Path (Join-Path $DataDir 'backup\original_scheme.json'))
Assert-Row '3' 'a preset was saved' `
  ((Test-Path (Join-Path $DataDir 'presets.json')) -and `
   ((Get-Content (Join-Path $DataDir 'presets.json') -Raw -ErrorAction SilentlyContinue).Trim() -notin @('', '[]')))
Assert-Row '3' 'a custom cursor was built' `
  ((Test-Path (Join-Path $DataDir 'custom')) -and `
   (@(Get-ChildItem (Join-Path $DataDir 'custom') -ErrorAction SilentlyContinue).Count -gt 0))

$applied = (Get-CursorValues)['Arrow']
Assert-Row '3' 'a cursor is applied' ($applied -like '*Cursed*') $applied

# The fingerprint the whole exercise turns on.
Assert-Row '3' 'the data directory was fingerprinted' (Get-DataPrint $beforePrint) $beforePrint

$versionBefore = Get-InstalledVersion
Write-Host "  version before the update: $versionBefore" -ForegroundColor DarkGray

# --- 4. the update -----------------------------------------------

Section "4. The update, $versionBefore -> $To"

if (-not $state.updated) {
  Pause-For @"
In the app: Settings -> CHECK FOR UPDATES -> DOWNLOAD UPDATE -> INSTALL & RESTART.

Watch for, and write down, anything you see:
  - more than ONE progress bar
  - any page asking whether to uninstall first
  - any question about keeping your presets and custom cursors
  - the app NOT coming back on its own
  - a second desktop shortcut appearing

If the app does not relaunch itself, start it by hand before continuing.
"@
  $state | Add-Member -NotePropertyName updated -NotePropertyValue $true -Force
  Save-State $state
}

$versionAfter = Get-InstalledVersion
Assert-Row '4' "the installed version is now $To" ($versionAfter -eq $To) "reports '$versionAfter'"

$running = @(Get-Process -Name 'Cursed' -ErrorAction SilentlyContinue)
Assert-Row '4' 'the app is running after the update' ($running.Count -gt 0) "$($running.Count) process(es)"
Assert-Row '4' 'exactly one copy is running' ($running.Count -le 1) "$($running.Count) process(es)"

$desktopShortcuts = @(Get-ChildItem "$env:USERPROFILE\Desktop" -Filter 'Cursed*.lnk' -ErrorAction SilentlyContinue)
Assert-Row '4' 'no duplicate desktop shortcut' ($desktopShortcuts.Count -le 1) `
  ($desktopShortcuts.Name -join ', ')

# --- 5. the headline: the data survived --------------------------

Section '5. The data'

Assert-Row '5' 'the data directory still exists' (Test-Path $DataDir)
Assert-Row '5' 'the original-scheme snapshot survived' `
  (Test-Path (Join-Path $DataDir 'backup\original_scheme.json'))
Assert-Row '5' 'presets survived' (Test-Path (Join-Path $DataDir 'presets.json'))
Assert-Row '5' 'settings survived' (Test-Path (Join-Path $DataDir 'settings.json'))
Assert-Row '5' 'custom cursors survived' `
  ((Test-Path (Join-Path $DataDir 'custom')) -and `
   (@(Get-ChildItem (Join-Path $DataDir 'custom') -ErrorAction SilentlyContinue).Count -gt 0))

if (Get-DataPrint $afterPrint) {
  $before = Get-Content $beforePrint -Raw | ConvertFrom-Json
  $after  = Get-Content $afterPrint  -Raw | ConvertFrom-Json

  $identical = $before.digest -eq $after.digest
  Assert-Row '5' 'the data directory is byte-identical' $identical `
    "before $($before.digest.Substring(0,12)), after $($after.digest.Substring(0,12))"

  if (-not $identical) {
    # Not every difference is data loss -- the app relaunched and may have saved
    # a window position -- so the four that cannot be re-made are named
    # individually.
    $irreplaceable = @('backup\original_scheme.json', 'presets.json', 'settings.json', 'applied.json')
    foreach ($name in $irreplaceable) {
      $key = $name.ToLower()
      $was = $before.entries | Where-Object { $_.path -eq $key }
      $now = $after.entries  | Where-Object { $_.path -eq $key }
      if (-not $was) { continue }
      $ok = $now -and ($now.sha256 -eq $was.sha256)
      Assert-Row '5' "$name is unchanged" $ok $(if (-not $now) { 'DELETED' } else { 'modified' })
    }

    Write-Host "`n  everything that differs:" -ForegroundColor Yellow
    $beforeMap = @{}; foreach ($e in $before.entries) { $beforeMap[$e.path] = $e.sha256 }
    $afterMap  = @{}; foreach ($e in $after.entries)  { $afterMap[$e.path]  = $e.sha256 }
    foreach ($path in ($beforeMap.Keys + $afterMap.Keys | Sort-Object -Unique)) {
      if (-not $afterMap.ContainsKey($path))       { Write-Host "    removed  $path" -ForegroundColor Yellow }
      elseif (-not $beforeMap.ContainsKey($path))  { Write-Host "    added    $path" -ForegroundColor DarkGray }
      elseif ($beforeMap[$path] -ne $afterMap[$path]) { Write-Host "    changed  $path" -ForegroundColor Yellow }
    }
  }
} else {
  Add-Row '5' 'the data directory is byte-identical' 'skip' 'the fingerprint could not be taken'
}

# --- 6. all seventeen roles still resolve ------------------------

Section '6. Pointer roles after the update'

$now = Get-CursorValues
$missing = @()
foreach ($role in $Roles) {
  $value = $now[$role]
  if ([string]::IsNullOrEmpty($value)) { continue }
  $expanded = [Environment]::ExpandEnvironmentVariables($value)
  if (-not (Test-Path $expanded)) { $missing += "$role -> $expanded" }
}
Assert-Row '6' 'every set role points at a file that exists' ($missing.Count -eq 0) ($missing -join '; ')
Assert-Row '6' 'the applied cursor is still ours' ((Get-CursorValues)['Arrow'] -like '*Cursed*') `
  (Get-CursorValues)['Arrow']

# --- 7. uninstall ------------------------------------------------

Section '7. Uninstall'

if (-not $state.uninstalled) {
  Pause-For @"
Uninstall Cursed the way a user would: Settings -> Apps -> Cursed -> Uninstall.

When it asks whether to keep your presets and custom cursors, choose NO --
this step is verifying that a full removal really is full.
"@
  $state | Add-Member -NotePropertyName uninstalled -NotePropertyValue $true -Force
  Save-State $state
}

$verifyUninstall = Join-Path $PSScriptRoot 'verify-uninstall.ps1'
if (Test-Path $verifyUninstall) {
  Write-Host '  handing over to verify-uninstall.ps1...' -ForegroundColor DarkGray
  & powershell -NoProfile -ExecutionPolicy Bypass -File $verifyUninstall -BaselinePath $baselinePath
  $uninstallFailures = $LASTEXITCODE
  Assert-Row '7' 'uninstall left nothing behind' ($uninstallFailures -eq 0) `
    "$uninstallFailures assertion(s) failed"
} else {
  Add-Row '7' 'uninstall left nothing behind' 'skip' 'verify-uninstall.ps1 not found beside this script'
}

# --- the table ---------------------------------------------------

$passed  = @($script:Rows | Where-Object { $_.Result -eq 'pass' }).Count
$failed  = @($script:Rows | Where-Object { $_.Result -eq 'fail' }).Count
$skipped = @($script:Rows | Where-Object { $_.Result -eq 'skip' }).Count

Write-Host "`n`nPaste everything below into docs/verification/update-path.md" -ForegroundColor White
Write-Host ('-' * 72)
Write-Host ""
Write-Host ("Run on {0}, {1} -> {2}, {3}" -f `
  (Get-Date).ToString('yyyy-MM-dd'), $versionBefore, $versionAfter, [Environment]::OSVersion.Version)
Write-Host ""
Write-Host "| # | Check | Result |"
Write-Host "| --- | --- | --- |"
foreach ($row in $script:Rows) {
  $mark = switch ($row.Result) { 'pass' { 'PASS' } 'fail' { '**FAIL**' } default { 'skipped' } }
  $detail = if ($row.Detail) { " -- $($row.Detail)" } else { '' }
  Write-Host ("| {0} | {1}{2} | {3} |" -f $row.Section, $row.What, $detail, $mark)
}
Write-Host ""
Write-Host ("{0} passed, {1} failed, {2} skipped." -f $passed, $failed, $skipped)
Write-Host ('-' * 72)

if ($failed -eq 0 -and $skipped -eq 0) {
  Write-Host "`nClean run." -ForegroundColor Green
} elseif ($failed -eq 0) {
  Write-Host "`nNo failures, but $skipped row(s) could not be checked. A skip is not a pass." -ForegroundColor Yellow
} else {
  Write-Host "`n$failed row(s) failed. Do not publish." -ForegroundColor Red
}

Remove-Item $StatePath -ErrorAction SilentlyContinue
exit $failed
