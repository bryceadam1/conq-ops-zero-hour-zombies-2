<#
  Real-input smoke test for Conquer Ops (Windows; needs `um`, like autotest.ps1).

  Starts the staged build with ZHBO_AUTOPILOT=manual: the test scene is set up and the General
  enters first person, but nothing is scripted, so movement and look come from real OS input
  (SDL3 in the 64-bit build, Win32 in the DX8 build). Holds W, then sends relative mouse motion,
  and checks the per-second pose lines in ConquerOps.log for a position change and a yaw change.
#>
param(
    [string]$Dist = "$env:LOCALAPPDATA\ConquerOps\dist",
    [string]$Out = "$env:TEMP\ConquerOps-inputtest",
    [double]$Scale = 0.5
)
$ErrorActionPreference = "Stop"
$log = Join-Path ([Environment]::GetFolderPath('MyDocuments')) "Command and Conquer Generals Zero Hour Data\ConquerOps.log"
New-Item -ItemType Directory -Force $Out | Out-Null
function P([int]$x, [int]$y) { "{0} {1}" -f [int]($x * 2 * $Scale), [int]($y * 2 * $Scale) }

Get-Process generalszh -ErrorAction SilentlyContinue | ForEach-Object { Stop-Process -Id $_.Id -Force }
$env:ZHBO_AUTOPILOT = "manual"
Start-Process -FilePath (Join-Path $Dist "ConquerOps.exe") -ArgumentList "-win", "-xres", "1280", "-yres", "720", "-noshellmap"
Start-Sleep 25

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
Start-Sleep 2

function Pose {
    $line = (Get-Content $log | Select-String "pos \(" | Select-Object -Last 1).Line
    if ($line -match "pos \(([-\d.]+), ([-\d.]+), [-\d.]+\) yaw ([-\d.]+)") {
        return @{ x = [double]$Matches[1]; y = [double]$Matches[2]; yaw = [double]$Matches[3] }
    }
    throw "no pose line in $log"
}

$before = Pose
um win drive --proc generalszh "focus" "scanmode on" "hold 0x57 2500" "sleep 1500" | Out-Null
$moved = Pose
um win drive --proc generalszh "focus" "rel 200 0" "sleep 50" "rel 200 0" "sleep 50" "rel 200 0" "sleep 1500" | Out-Null
$turned = Pose
um win shot --exe generalszh.exe (Join-Path $Out "after-input.png") --scale 0.5 | Out-Null
Copy-Item $log (Join-Path $Out "ConquerOps.log")
Get-Process generalszh -ErrorAction SilentlyContinue | ForEach-Object { Stop-Process -Id $_.Id -Force }

$dist = [math]::Sqrt([math]::Pow($moved.x - $before.x, 2) + [math]::Pow($moved.y - $before.y, 2))
$dyaw = [math]::Abs($turned.yaw - $moved.yaw)
"{0,-22} {1} ({2:N1} units)" -f "W moves the General", $(if ($dist -gt 5) { "PASS" } else { "FAIL" }), $dist
"{0,-22} {1} ({2:N2} rad)" -f "mouse turns the view", $(if ($dyaw -gt 0.05) { "PASS" } else { "FAIL" }), $dyaw
"screenshot and log: $Out"
