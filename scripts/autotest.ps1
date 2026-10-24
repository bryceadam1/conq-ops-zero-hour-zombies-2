<#
  In-game smoke test for Conquer Ops.

  Starts the staged build through ConquerOps.exe with ZHBO_AUTOPILOT=1, clicks through
  Solo Play -> Skirmish -> Play Game (default skirmish settings), waits for the autopilot to take
  control of the spawned General, and saves timed screenshots plus ConquerOps.log to -Out.

  Needs `um` (universal-modder) for window input and capture: `uv tool install git+https://github.com/rehan-remade/universal-modder`
  then `um win setup`. Takes over mouse/keyboard input to the game window for about 20 seconds.

  Click coordinates are client coordinates of a 1280x720 game window at 200% Windows scaling
  (the window reports 640x360). Pass -Scale 1 on a 100% scaled desktop.
#>
param(
    [string]$Dist = "$env:LOCALAPPDATA\ConquerOps\dist",
    [string]$Out = "$env:TEMP\ConquerOps-autotest",
    [double]$Scale = 0.5
)
$ErrorActionPreference = "Stop"
$log = Join-Path ([Environment]::GetFolderPath('MyDocuments')) "Command and Conquer Generals Zero Hour Data\ConquerOps.log"
New-Item -ItemType Directory -Force $Out | Out-Null
function P([int]$x, [int]$y) { "{0} {1}" -f [int]($x * 2 * $Scale), [int]($y * 2 * $Scale) }

Get-Process generalszh -ErrorAction SilentlyContinue | ForEach-Object { Stop-Process -Id $_.Id -Force }
$env:ZHBO_AUTOPILOT = "1"
Start-Process -FilePath (Join-Path $Dist "ConquerOps.exe") -ArgumentList "-win", "-xres", "1280", "-yres", "720", "-noshellmap"
Start-Sleep 25

# Skip the intro, then Solo Play -> Skirmish -> Play Game. Startup time varies, so retry the menu
# sequence until the autopilot reports that the General is in first person.
$entered = $false
for ($attempt = 0; $attempt -lt 3 -and -not $entered; $attempt++) {
    um win drive --proc generalszh "focus" "scanmode on" "key 0x1B" "sleep 1500" "key 0x1B" "sleep 8000" `
        "click $(P 514 80)" "sleep 3000" "click $(P 514 175)" "sleep 4000" "click $(P 145 318)" | Out-Null
    for ($i = 0; $i -lt 120; $i++) {
        if ((Get-Content $log -ErrorAction SilentlyContinue) -match "entered first-person") { $entered = $true; break }
        Start-Sleep -Milliseconds 500
    }
}
if (-not $entered) { throw "the General never entered first person; see $log" }

# Autopilot timeline (seconds in first person): walk 1-3, turn 3-4, aim 4.5-8, fire 5-7.5, crouch, jump 10, field computer 11.
foreach ($shot in @(@(2.5, "walk"), @(2.3, "aim"), @(1.0, "fire"), @(0.8, "fire-2"), @(1.9, "reload"), @(2.6, "field-computer"), @(2.0, "rts"), @(5.0, "re-entry"))) {
    Start-Sleep $shot[0]
    um win shot --exe generalszh.exe (Join-Path $Out "$($shot[1]).png") --scale 0.5 | Out-Null
}
Copy-Item $log (Join-Path $Out "ConquerOps.log")
Get-Process generalszh -ErrorAction SilentlyContinue | ForEach-Object { Stop-Process -Id $_.Id -Force }

$text = Get-Content (Join-Path $Out "ConquerOps.log") -Raw
$checks = [ordered]@{
    "entered first person"    = $text -match "entered first-person control"
    "moved"                   = $text -match "pos \("
    "killed a target"         = $text -match "killed a target"
    "field computer"          = $text -match "opened the field computer"
    "returned to RTS"         = $text -match "returned General \d+ to RTS control"
    "re-entered"              = $text -match "taking control again"
    "took enemy fire"         = $text -match "General took \d+ damage"
    # R8d: impacts land (flesh on the rebels, a terrain surface on the misses) and the bridge's
    # name lookup resolved and ran an FXList.
    "impacts: flesh"          = $text -match "player 0 impacts:.*flesh [1-9]"
    "impacts: terrain"        = $text -match "player 0 impacts:.*(concrete|dirt|grass|asphalt|rock|sand|snow|wood) [1-9]"
    "impact fx played"        = $text -match "impact fx: [1-9][0-9]* played, 0 missing"
}
$checks.GetEnumerator() | ForEach-Object { "{0,-22} {1}" -f $_.Key, ($(if ($_.Value) { "PASS" } else { "FAIL" })) }
"screenshots and log: $Out"
if ($checks.Values -contains $false) { exit 1 }
