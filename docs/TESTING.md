# Conquer Ops — Testing

How each layer of the mod is verified, and what to run before declaring a change done.

## Layers

| Layer | What it covers | How |
|---|---|---|
| Unit tests | sim, weapons, movement, hitscan, HUD, formats, requirements | `cargo test --workspace` |
| Headless renderer | frame encoding, transient and dynamic buffers, read-back pixels, draw validation, ABI asserts | `cargo test -p zhbo-gfx` (`tests/headless.rs`, `tests/ffi.rs`) |
| Asset conformance | BO2/ConquerOps zones load with 0 bytes left over; weapon table round-trips | `cargo test -p zhbo-assets` (BO2 install needed for the `#[ignore]`d tests) |
| In-game scripted test | full loop: take control, move, fight, field computer, RTS hand-back | `scripts/autotest.ps1` (Windows) / `scripts/autotest-mac.sh` (macOS) |
| Real input test | OS keyboard/mouse path the autopilot does not cover | `scripts/inputtest.ps1` / `autotest-mac.sh --input` |

## Autopilot and the input path

The autopilot replaces all input inside the Rust core, so `autotest.ps1` never exercises the OS
input path. `ZHBO_AUTOPILOT=manual` sets up the scene without scripted input, and
`scripts/inputtest.ps1` drives real keys and mouse through it.

Scripting real keys: `hold <VK> <ms>` needs a hex VK code (`hold 0x57 2500` for W); on SDL,
`scanmode on` switches `hold`/`press` to scan codes. macOS holds keys by code with
`osascript key code`, or `cliclick` for the mouse.

## macOS test notes

- The fork's `-loadsave` skips the menus, so macOS tests start straight into the save.
- `ConquerOps.log` survives between runs: delete it before launching, or a wait for
  "entered first-person" matches the previous run and input is sent during the loading screen.
- The macOS runner holds keys with `osascript key code` (mouse via `cliclick`); `hold` timing is
  looser than Windows' SendInput, so movement distances differ.
- Some saves end with the General dying before re-entry; the "re-entered" check then only proves
  the line is logged, not that first person came back.

## Windows notes

- Both builds (DX8 fork and wgpu) pass `autotest.ps1` and `inputtest.ps1`; run the DX8 build for
  the baseline and the wgpu build to compare rendering.
