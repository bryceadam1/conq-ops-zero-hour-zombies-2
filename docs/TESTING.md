# Conquer Ops — Testing

How each layer of the mod is verified, and what to run before declaring a change done.

## Layers

| Layer | What it covers | How |
|---|---|---|
| Unit tests | sim, weapons, movement, hitscan, HUD, formats, requirements | `cargo test --workspace` |
| Headless renderer | frame encoding, transient and dynamic buffers, read-back pixels, draw validation, ABI asserts, the native BO2 pass (skinning, sRGB, lighting, view order, refusals) | `cargo test -p zhbo-gfx` (`tests/headless.rs`, `tests/ffi.rs`, `tests/native.rs`) |
| Native viewmodel (Track R) | blob formats round-trip and refuse damage (`zhbo-native`); the core's request/answer loading, composition, posing and ADS slide without a GPU; end to end into a headless renderer | `cargo test -p zhbo-native`; `cargo test -p zhbo-core nativevm` and `cargo test -p zhbo-core --features renderer nativevm`; the real-install conversion test checks every converted viewmodel's blobs (`ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets converts_from_real_install -- --ignored`) |
| Native third person (Track R7) | the body and world-weapon manifests round-trip (`zhbo-native`); the clip resolver's state scan (`zhbo-assets`); the core's third-person composition, state selection and world placement without a GPU | `cargo test -p zhbo-native -p zhbo-assets -p zhbo-core nativebody`; the real-install conversion test additionally checks every faction body (72 inches, weapon tag, all states' clips, textures) composes with every world weapon (`ZHBO_BO2_DIR=… cargo test -p zhbo-assets converts_from_real_install -- --ignored`) |
| Asset conformance | BO2/ConquerOps zones load with 0 bytes left over; weapon table round-trips | `cargo test -p zhbo-assets` (BO2 install needed for the `#[ignore]`d tests) |
| In-game scripted test | full loop: take control, move, fight, throw both offhands, toggle the grenade launcher, field computer, RTS hand-back. The script seeds a v2 profile with one custom class in the active set — the AN-94 with its under-barrel launcher, plus the frag, concussion, pistol and knife — because BO2's premades carry no attachments and the alt-mode step needs one; the autopilot picks that custom from the spawn screen by its computed row key (the match list is the set's custom slots, then the defaults, `changeclass.lua`) | `scripts/autotest.ps1` (Windows) / `scripts/autotest-mac.sh` (macOS; seeds `ConquerOps.profile` with the GL custom before launch — the standard autopilot picks the first profile custom when there is one, else the first default class) |
| Real input test | OS keyboard/mouse path the autopilot does not cover | `scripts/inputtest.ps1` / `autotest-mac.sh --input` |
| Overhead General | his class guns swapped in (`armed slot 0 with ZHBO_GeneralWeapon_`), rounds counted, the command bar's orders given the way the buttons give them — the lethal at a rebel (thrown as first person throws), a reload, the pistol, the knife at a rebel spawned close (`meleed`) — then first person keeps the pistol. `--rts-dry` runs it with no spare rifle, then no spare pistol, rounds: the dry gun hands over | `autotest-mac.sh --rts` / `--rts-dry` (`ZHBO_AUTOPILOT=rts`; macOS only, `autotest.ps1` has no RTS mode) |
| Create-a-class run | the in-match screen is pick-only and shows the profile's ACTIVE class set: the script seeds a v2 profile with one custom (MP7 primary, claymore — deliberately unlike any premade kit) in set 1, picks it from the spawn screen by its row key (the custom leads the match list, `changeclass.lua`) and the sim logs `assumed the AB class`, with no profile write; the once-a-second pose line then names the MP7, proving the *kit* landed; it then leaves first person, takes control again, forces the screen open and Escapes it, and first-person play continues | `ZHBO_AUTOPILOT=classpick` with either autotest script (the macOS script seeds `ConquerOps.profile` first, then checks `assumed the AB class`, `MP7 <n>\|` in a pose line, no `create-a-class: saved` line, a second `entered first-person control`, and `taking control again`) |
| Shell create-a-class | the lobby's class screen (the menu ABI): browse, edit, save, delete, copy, close — in the shell, no match | manual only: the in-game autotest does not cover the shell (see below) |
| Class-screen screenshots | the shell list, the in-match pick list and the Pick-10 editor rendered through `classui::draw_screen` | `cargo run -p zhbo-assets -- screenshot` (uses the locally built `ConquerOps_BO2.big`; PNGs in `target/cac-shots/`) |

## The shell create-a-class screen (manual)

The lobby's Create a Class screen is a shell window driven through the menu ABI
(`docs/CONTRACT.md`); no scripted run covers it (the autotest loads straight into a match), so
verify it by hand after touching the menu ABI, the shell window, or the shared class-screen code:

1. Launch the game (`conquerops`), click through: **Solo Play → Skirmish**; the **Create a Class**
   button sits at the top-right of the lobby panel.
2. Click it: the BO2 class screen opens *in the shell* (menu background art, the class list, the
   kit panel). The lobby beneath is inert (its buttons and its Escape shortcut do not react).
3. Browse with the mouse (hover highlights, the wheel scrolls) and the arrows: the list is the
   active set's five custom slots (`cacchooseclass.lua`) — **Enter/click on a filled row** opens
   the editor on that class; on an untouched **`Custom n`** row opens a fresh draft. **Escape** in
   the editor returns to the list.
4. Edit something, walk to **SAVE CLASS**, confirm: `ConquerOps.profile` (in the game's user-data
   directory) is rewritten with the new class, and the list shows it highlighted.
5. On a custom row: **C** copies it (`NAME COPY` appears), **Backspace** deletes it — both rewrite
   the profile file.
6. **Escape** on the list closes back to the skirmish lobby (and the lobby's Escape works again).
7. Start a match: the saved class appears on the in-game pick screen (the set's five custom
   slots, then the default classes), which is **pick-only** (no editor, no delete/copy, untouched
   slots are inert — BO2 creates classes only in the lobby); the
   spawn offer opens in first person while unclassed, the command-bar button forces it, and Escape
   closes it and leaves the General unarmed (knife only) — the offer re-arms on the next
   take-control. `scripts/autotest-mac.sh --classpick` covers that flow.

Unit tests drive the same screen headlessly through `MenuState` (`crates/zhbo-core/src/menu.rs`:
browse → edit → save → file text out; delete; copy; close), and the FFI layer
(`crates/zhbo-core/src/ffi.rs`: open/frame/close, and `zhbo_reset` dropping the screen). The
command-bar class buttons are covered headlessly the same way: `client::tests` drives a
`ZhboClientIn.class_button` one-shot through `Client::frame` into `Sim::choose_class` and asserts
the kit lands (preset and profile custom, and that an empty slot sends nothing), and
`ffi::tests::a_command_bar_class_button_reaches_the_client_out_through_the_abi` covers the ABI hop.

## The HUD contact sheet (atlas forensics)

`cargo run -p zhbo-assets -- hud-sheet [out-dir] [--archive <ConquerOps_BO2.big>] [--cols N]` renders
EVERY MappedImage in the converted archive to one labelled PNG grid (`hud-sheet.png` plus
`hud-sheet.txt` with rects/opaque-% and the raw `atlas-page-N.png` pages). Use it when an icon looks
stitched or mis-framed: a tile showing two images means a bad rect or a page mix-up, and the sheet
says which. It reads the already-converted archive (no BO2 install needed, never launches the game).

## Headless class-screen screenshots

`cargo run -p zhbo-assets -- screenshot [out-dir] [--archive <ConquerOps_BO2.big>] [--size WxH]`
renders the class screens through the same drawing code the game runs (`classui::draw_screen`) and
writes nine shots: `cac-browse.png` (the lobby list: the active set's five custom slots, the set
header and the S/O footer), `cac-set-pager.png` (a browsed mid-row set), `cac-pick.png` (the
in-match list: the slots, then the default classes), `cac-editor.png` (a populated Pick-10 draft
with the primary option list open), `cac-editor-empty.png` (BO2's pristine empty class),
`cac-editor-weapons.png` (the category column and stat bars), `cac-editor-attachments.png` (the
attachment strip) and `cac-dialog-set.png`/`cac-dialog-class.png` (the S/O dialogs). Default size
is 1280x720 (BO2's 720p; the captures are 3024x1890). The default output is `target/cac-shots/`,
which is inside the ignored build directory.

It reads the player's already-converted `ConquerOps_BO2.big` (the launcher's normal output; found in
the dist folder, or pass `--archive`/set `ZHBO_BO2_ARCHIVE`) and the `weapons.ini` inside it, so it
needs no Black Ops II install and never launches the game. Sprites resolve through the archive's
MappedImage INI and TGA pages; art the archive lacks draws its fallback prims exactly as
`hud::render` does, and the harness prints those sprite names. Use it to iterate on layout and art
without starting a match; the unit tests cover the same views with fallbacks only (no archive).

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
