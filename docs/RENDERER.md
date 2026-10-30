# Renderer: the engine's bgfx API on wgpu

Conquer Ops builds Zero Hour from bobtista's fork of the SuperHackers source. In that fork, the
original D3D8 renderer sits behind `IRenderBackend`, and `BgfxBackend` implements it on a subset of
the [bgfx](https://github.com/bkaradzic/bgfx) API. We keep `BgfxBackend` unchanged and replace bgfx
itself with **zhbo-gfx**, a Rust implementation of that subset on [wgpu](https://wgpu.rs). wgpu
drives D3D12, Vulkan, Metal or GL, so one renderer serves Windows, Linux and macOS, with no Wine.

```
BgfxBackend.cpp (engine, unchanged)
   │  bgfx:: calls, declared by bgfx's own bgfx.h
   ▼
host/zhbo/bgfx_wgpu.cpp     C++ adapter: bgfx structs, Memory blocks, transient buffers
   │  zhgfx_* C ABI (host/zhbo/zhbo_gfx.h)
   ▼
crates/zhbo-gfx             Rust: runtime (views, draws, resources) on wgpu
```

## Build

`cargo xtask build --wgpu` (Windows for now; macOS and Linux follow in M6):

1. `zhbo-core` is built for `x86_64-pc-windows-msvc` with `--features renderer`, which links
   zhbo-gfx and its `zhgfx_*` functions into the static library the game already links.
2. `zhbo-shaderc` is built for the build machine.
3. The engine is configured with the fork's `win64` preset (SDL3, OpenAL, 64-bit) plus
   `GGC_BGFX_RENDERER=wgpu` and `ZHBO_SHADERC=<path>`. `host/patches/0002-wgpu-renderer.patch` adds
   that renderer value. With it set, the build:
   * runs `zhbo-shaderc` instead of bgfx's shaderc. It takes shaderc's arguments and writes the same
     `--bin2c` headers, named with the default `_dx11` suffix the backend includes;
   * links `zhbo_bgfx_wgpu` (the adapter, defined in `host/zhbo/CMakeLists.txt`) instead of the bgfx
     library. bgfx's headers and bx/bimg still come from the engine's pinned `bgfx.cmake`.
4. The `win64` preset's OpenAL audio decodes with FFmpeg. xtask downloads the build the fork's CI
   uses, BtbN's FFmpeg 8.1 win64 shared build, once into `<engine>/.deps-cache/ffmpeg-x64`. That is
   where the preset points `FFMPEG_ROOT`. bgfx.cmake's patch step needs `sh`, which xtask adds to
   PATH from Git for Windows.
5. The SDL3, OpenAL and FFmpeg DLLs are copied next to `generalszh.exe`.

The renderer logs to `zhbo-gfx.log` next to the executable: the adapter, fallbacks taken, and the
first 32 wgpu errors in full. Lines naming a problem are also passed to the engine's bgfx callback
as `WARN`.

## Shaders

The engine's shaders are bgfx `.sc` files. `crates/zhbo-gfx/src/shader.rs` rewrites them into
Vulkan-style GLSL 450. naga then validates them and emits WGSL. All 27 engine variants are
translated in a unit test.

* Loose uniforms go into one std140 block per stage, `@group(0)`: binding 0 for the vertex stage,
  binding 1 for the fragment stage. The two stages have different layouts, as with bgfx's per-stage
  constant buffers.
* Only the predefined uniforms a shader references are emitted, such as `u_modelViewProj` and
  `u_alphaRef4`. The runtime fills them at `frame()`, once view transforms are final.
* `SAMPLER2D(name, slot)` becomes a texture at `@group(1) @binding(2*slot)` and a sampler at
  `2*slot + 1`.
* Attribute locations come from semantics: POSITION 0, NORMAL 1, COLOR0 4, TEXCOORDn 10+n. Instance
  data `i_dataN` uses bgfx's TEXCOORD7, then 6, 5 and 4. Varyings are numbered in `varying.def.sc`
  order.
* `BGFX_SHADER_LANGUAGE_GLSL` is 0. wgpu follows D3D and Metal conventions (depth 0..1, top-left
  texture origin), and the adapter reports `homogeneousDepth = false` and `originBottomLeft = false`.

The header array holds a **shader blob**: `ZHS1`, the stage, the uniform, sampler and attribute
reflection, then the WGSL (`src/blob.rs`). `createShader` decodes it.

## Runtime model (`src/runtime/`)

It follows bgfx's semantics:

* **Views 0–255** have a rect, a clear, a mode, a frame buffer and view/proj matrices. `submit`
  records a draw into a view. `frame()` executes views in `setViewOrder` order. Each view that has
  draws, or was touched, becomes one render pass.
* **Sticky draw state** (`setState`, `setStencil`, transforms, buffers, textures) is captured at
  `submit` and then discarded according to the `BGFX_DISCARD_*` flags.
* **Uniform values are sticky.** Each draw's user uniforms are written into a per-frame ring with
  dynamic offsets.
* **Resource creation and updates go to the queue immediately.** The queue runs them before the
  frame's command buffer, which matches bgfx's "updates land before the frame" order. Draws hold
  clones of the wgpu objects, so destroying a handle mid-frame is safe.
* **Transient buffers** are fixed CPU arenas the engine writes into, uploaded at `frame()`.
* **Partial clears**, where a view rect is smaller than its target, are drawn as a quad, as bgfx does.
* **The game renders into an offscreen BGRA8 backbuffer**, which is then copied to the window
  surface. This gives MSAA, `requestScreenShot`, headless runs and a fixed format on every platform.

State words are decoded in `src/state.rs`: blend, depth, stencil, cull, primitive type, sampler
flags and texture formats, using bit layouts from bgfx's `defines.h`.

### The native BO2 model pass (`src/runtime/native.rs`, `native.wgsl`)

Besides the engine's bgfx draws, the runtime draws BO2 models itself (Track R): the core uploads a
skinned mesh once (`native_create_mesh`: 56-byte vertices with four bone influences, `u16`
indices, surfaces with BO2's blend, emissive flag, colour map, packed normal/gloss/specular map
and specular colour map from `native_create_texture`, which builds the mip chain; both upload raw — see shading) and
queues a draw per frame with its bone palette (`native_submit`, at most 256 bones). Queued draws
run in one pass immediately before their `before_view` (the scene composite), into the
`target_view`'s frame buffer and rect; a draw whose before-view never comes runs at the end of the
frame. A `None` `view_proj` takes the target view's own view × projection at `frame()`. The pass
has its own `Depth32Float` buffer, cleared and discarded, so the viewmodel draws over the world
and its parts sort among themselves; opaque surfaces draw first, blended ones after in BO2's
sort-key order, and each surface culls the faces its material's `cullFace` says to
(`GFXS0_CULL_*`, bits 14-15 of `GfxStateBits::loadBits[0]` — ordinary lit surfaces cull their
back faces, cutouts and glows draw double-sided; the pass keeps BO2's index order, so front is
clockwise: the engine's content winds the other way, which is why the W3D conversion swaps
indices). Shading is `bo2Shade` (host patch 0009) ported to WGSL **in display (gamma)
space**, as bo2Shade itself runs and as the rest of Zero Hour's scene is lit: colour maps upload
raw, the lights are Zero Hour's display-encoded values used as-is, and the shaded result is
written straight to the gamma-space scene target — unless that target is itself sRGB, in which
case the result is decoded to linear first (the hardware re-encodes on write). A linear-light
port washed dark finishes out to grey: bo2Shade's environment term, tuned in gamma space, ends up
several times stronger than a dark albedo's diffuse when both are decoded to linear (MODLOG,
Track R round 2). Bad meshes and draws are refused, logged once and counted in
`stats().native_skipped`; they never reach wgpu validation. A surface with no indices is not bad:
it is a no-op draw (a surface the compose step's hide tags emptied), so it is skipped and logged and
the mesh's other surfaces draw; only a malformed count or a range past the index buffer refuses the
mesh, and a mesh left with no drawable surface at all is refused so the W3D fallback draws.

**The camo layer (PLAN D1/E).** A native surface may carry a second texture: `NativeSurface.camo`
(the chosen pattern/solid image, `INVALID` for none) and `camo_uv = [offset.x, offset.y, scale,
gain]`. The material bind group has binding 4 (the camo texture; the 1×1 white page is bound when
absent) and the material uniform carries `camo: vec4` after `params: vec4`. `native.wgsl` samples
`camo_map` unconditionally at `uv * scale + offset` (implicit derivatives need uniform control
flow) and, when `params.w > 0.5`, replaces the albedo: `mix(albedo.rgb, camo.rgb * gain, camo.a)` — the
layer's alpha is its coverage on the converted art, and `gain` is per set (2.0 from the C0 probe;
1.0 is the fallback). A stock surface binds white and skips the mix. Style camos are material
replacements, not a layer: `nativevm::camo::apply` swaps the surface's
colour/maps/specular/blend/emissive/sort/cull to the converted style material.

**The specular colour.** BO2's spec maps are colour maps (`*_spc-rgb`): a metal's hue lives in its
specular, not its colour map (the gold camo's colour map is a flat 21/20/13, its spec map mean
158/135/83; brass cartridges, tritium vials and lens coatings are tinted the same way).
`NativeSurface.specular` (binding 5; the 1×1 white page when absent) carries the converted map
(`*_spec.tga`, the spec map's RGB at its own size, raw), and the material uniform is 48 bytes
(`params`, `camo`, `spec: vec4` whose `x` flags the map). The reflectance at normal incidence is
then a colour, `f0 = spec.rgb²` — the same square the packed maps' scalar level (`maps.a²`) always
used — so the highlight, the environment reflection and the diffuse energy split `(1 − f0)` take
the hue. A grey spec map draws exactly as its level; a surface without the map (or without packed
maps, whose gloss it needs) keeps the level.

### Robustness

One invalid draw in a wgpu render pass invalidates the whole frame. The runtime therefore:

* checks index, vertex and instance ranges and offset alignment before encoding a draw, and skips
  any draw that fails, reporting it once;
* creates pipelines inside a validation error scope and caches failures, so the draws that need a
  failed pipeline are skipped;
* routes uncaptured wgpu errors to the engine's bgfx callback log, at most 32 per run.

### Adapters and fallbacks

* Adapters are ranked by hardware vs. software, then BC (DXT) support, then native API vs. GL, then
  discrete GPU. wgpu's default choice ignores BC.
* Without BC sampling (for example a VM's GL driver), BC1–3 textures are **decoded on the CPU** to
  RGBA8.
* D3D's 16-bit formats (R5G6B5, A1R5G5B5, A4R4G4B4) and A8 have no wgpu equivalent. They are expanded
  to 8-bit on upload.
* Without view-format reinterpretation (GL), sRGB textures are created as sRGB outright.
* Whether a format can be a render target is probed on the device, inside an error scope, once per
  format. On GL ES-class devices R32F cannot be rendered to. There, render targets fall back to the
  first renderable format from R32F, RGBA32F, R16F, RGBA16F and RGBA8; the shaders read `.r`.
  Parallels' GL driver renders none of the float formats, so its shadow and scene-depth targets
  become RGBA8.
* Dynamic buffers keep a CPU shadow copy. bgfx allows updates at any element offset, but
  `write_buffer` needs 4-byte alignment, so the enclosing aligned range is uploaded. Growing a
  buffer keeps its contents.
* Environment variables:
  * `ZHBO_GFX_BACKEND=dx12|vulkan|metal|gl` forces an API;
  * `ZHBO_GFX_FALLBACK=1` prefers the software adapter (WARP or lavapipe);
  * `WGPU_VALIDATION`, `WGPU_DEBUG`: see wgpu's `InstanceFlags::with_env`.

## Testing

* `cargo test -p zhbo-gfx` covers state decoding, BC decoding, matrices, blob round-trips, the
  27-variant engine shader translation, and two test files:
* `tests/headless.rs` draws frames through the public runtime API: transient and static buffers,
  a BC1 and an R5G6B5 texture, a model transform, a partial clear and alpha blending, dynamic
  buffer updates and grow (ALLOW_RESIZE), view reordering, plain and MSAA offscreen frame
  buffers, an sRGB-sampled texture and instance data — plus negative-path tests for draw
  validation (skip-and-report on bad handles, counts, layers and blit regions). It reads pixels
  back through the screenshot path and checks them. It passes on D3D12 WARP and on Parallels'
  GL driver (M1). It skips when no adapter exists; CI installs lavapipe for it.
* `tests/native.rs` drives the native BO2 pass headless: bone-skinned quads move with their
  palette, an emissive colour comes through exactly, a dark BO2 finish keeps its hue under Zero
  Hour's day lights and stays dark under a bright map ambient (the grey wash-out regression),
  light brightens a lit surface, surfaces cull the faces their materials say to (BO2's winding is
  clockwise-front here), a `None` matrix takes the view's transform, the pass runs before its
  view (or at the end), and every refusal is counted without a wgpu error.
* `tests/ffi.rs` locks the Rust side of the ABI: it mirrors the header's struct layouts and the
  API-table field counts with compile-time asserts and round-trips the encode functions through
  a `Vec<u8>` cursor, so header and Rust drift fail a plain `cargo test`.

## Engine integration notes

* `BgfxBackend` sends every 2D draw (identity view, orthographic projection) to its UI view, which
  runs after the scene composite. The mod's first-person sky therefore cannot be a 2D gradient
  painted before the 3D pass, as in the DX8 build. In wgpu builds the bridge instead sets the scene
  view's clear colour to the sky colour while in first person.

## Not done yet

* Render-target-sized textures (`BackbufferRatio`) are not resized on `reset`.
* Occlusion queries, indirect draws, compute, `setViewScissor` and `setScissor` are not
  implemented. The backend does not call them.
* `getStats` reports draw and blit counts and CPU frame time, but no GPU timings.
