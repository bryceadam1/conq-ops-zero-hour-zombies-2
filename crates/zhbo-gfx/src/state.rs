//! Decoding bgfx's packed state words into wgpu descriptors.
//!
//! The engine's render backend speaks bgfx: one `u64` render state per draw, a `u32` stencil word
//! per face, `u32` sampler flags per texture binding and `bgfx::TextureFormat` enum values. The
//! bit layouts here are bgfx's `defines.h` and `bgfx.h` at the pinned commit
//! (b2a7d94d574c6b82b54b6e8631cb6faafe807112). Everything in this module is pure so it can be
//! tested without a GPU.

use wgpu::{
    AddressMode, BlendComponent, BlendFactor, BlendOperation, BlendState, ColorWrites, CompareFunction, Face,
    FilterMode, FrontFace, MipmapFilterMode, PrimitiveTopology, StencilFaceState, StencilOperation, TextureFormat,
};

// --- render state (u64) -----------------------------------------------------------------------

pub const STATE_WRITE_R: u64 = 0x1;
pub const STATE_WRITE_G: u64 = 0x2;
pub const STATE_WRITE_B: u64 = 0x4;
pub const STATE_WRITE_A: u64 = 0x8;
pub const STATE_WRITE_Z: u64 = 0x40_0000_0000;
pub const STATE_DEPTH_TEST_SHIFT: u32 = 4;
pub const STATE_BLEND_SHIFT: u32 = 12;
pub const STATE_BLEND_EQUATION_SHIFT: u32 = 28;
pub const STATE_BLEND_INDEPENDENT: u64 = 0x4_0000_0000;
pub const STATE_BLEND_ALPHA_TO_COVERAGE: u64 = 0x8_0000_0000;
pub const STATE_CULL_SHIFT: u32 = 36;
pub const STATE_FRONT_CCW: u64 = 0x80_0000_0000;
pub const STATE_ALPHA_REF_SHIFT: u32 = 40;
pub const STATE_PT_SHIFT: u32 = 48;
pub const STATE_MSAA: u64 = 0x0100_0000_0000_0000;

/// Everything a draw's `u64` state decides about the pipeline. Two draws with equal
/// `PipelineState`s (and equal shaders, layouts and targets) share a pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PipelineState {
    pub write_mask: ColorWrites,
    pub depth_write: bool,
    /// `None` when depth testing is off (the draw still writes depth if `depth_write`).
    pub depth_compare: Option<CompareFunction>,
    pub blend: Option<BlendState>,
    pub cull_mode: Option<Face>,
    pub front_face: FrontFace,
    pub topology: PrimitiveTopology,
    pub alpha_to_coverage: bool,
    pub msaa: bool,
}

/// The alpha-test reference in `[0, 255]`. It is not pipeline state: bgfx feeds it to shaders
/// through the predefined `u_alphaRef4` uniform.
pub fn alpha_ref(state: u64) -> u8 {
    ((state >> STATE_ALPHA_REF_SHIFT) & 0xFF) as u8
}

fn compare(code: u64) -> Option<CompareFunction> {
    Some(match code {
        1 => CompareFunction::Less,
        2 => CompareFunction::LessEqual,
        3 => CompareFunction::Equal,
        4 => CompareFunction::GreaterEqual,
        5 => CompareFunction::Greater,
        6 => CompareFunction::NotEqual,
        7 => CompareFunction::Never,
        8 => CompareFunction::Always,
        _ => return None,
    })
}

fn blend_factor(code: u64) -> BlendFactor {
    match code {
        1 => BlendFactor::Zero,
        3 => BlendFactor::Src,
        4 => BlendFactor::OneMinusSrc,
        5 => BlendFactor::SrcAlpha,
        6 => BlendFactor::OneMinusSrcAlpha,
        7 => BlendFactor::DstAlpha,
        8 => BlendFactor::OneMinusDstAlpha,
        9 => BlendFactor::Dst,
        10 => BlendFactor::OneMinusDst,
        11 => BlendFactor::SrcAlphaSaturated,
        12 => BlendFactor::Constant,
        13 => BlendFactor::OneMinusConstant,
        _ => BlendFactor::One,
    }
}

fn blend_component(src: u64, dst: u64, equation: u64) -> BlendComponent {
    let operation = match equation {
        1 => BlendOperation::Subtract,
        2 => BlendOperation::ReverseSubtract,
        3 => BlendOperation::Min,
        4 => BlendOperation::Max,
        _ => BlendOperation::Add,
    };
    // WebGPU requires both factors to be One for min/max; D3D and GL ignore them there anyway.
    let (src_factor, dst_factor) = match operation {
        BlendOperation::Min | BlendOperation::Max => (BlendFactor::One, BlendFactor::One),
        _ => (blend_factor(src), blend_factor(dst)),
    };
    BlendComponent { src_factor, dst_factor, operation }
}

pub fn decode_state(state: u64) -> PipelineState {
    let mut write_mask = ColorWrites::empty();
    for (bit, w) in [
        (STATE_WRITE_R, ColorWrites::RED),
        (STATE_WRITE_G, ColorWrites::GREEN),
        (STATE_WRITE_B, ColorWrites::BLUE),
        (STATE_WRITE_A, ColorWrites::ALPHA),
    ] {
        if state & bit != 0 {
            write_mask |= w;
        }
    }

    let blend_bits = (state >> STATE_BLEND_SHIFT) & 0xFFFF;
    let blend = (blend_bits != 0).then(|| {
        let f = |shift: u32| (state >> shift) & 0xF;
        let equation = state >> STATE_BLEND_EQUATION_SHIFT;
        BlendState {
            color: blend_component(f(12), f(16), equation & 0x7),
            alpha: blend_component(f(20), f(24), (equation >> 3) & 0x7),
        }
    });

    let cull_mode = match (state >> STATE_CULL_SHIFT) & 0x3 {
        // bgfx's cull flags name the screen-space winding to discard, independent of FRONT_CCW.
        // Pipelines are built with a counter-clockwise front face, so CW is the back face.
        1 => Some(Face::Back),
        2 => Some(Face::Front),
        _ => None,
    };

    let topology = match (state >> STATE_PT_SHIFT) & 0x7 {
        1 => PrimitiveTopology::TriangleStrip,
        2 => PrimitiveTopology::LineList,
        3 => PrimitiveTopology::LineStrip,
        4 => PrimitiveTopology::PointList,
        _ => PrimitiveTopology::TriangleList,
    };

    PipelineState {
        write_mask,
        depth_write: state & STATE_WRITE_Z != 0,
        depth_compare: compare((state >> STATE_DEPTH_TEST_SHIFT) & 0xF),
        blend,
        cull_mode,
        front_face: FrontFace::Ccw,
        topology,
        alpha_to_coverage: state & STATE_BLEND_ALPHA_TO_COVERAGE != 0,
        msaa: state & STATE_MSAA != 0,
    }
}

/// The depth comparison to put in the pipeline. bgfx with no depth test but `WRITE_Z` still
/// writes depth, which in wgpu is `Always` with writes on.
pub fn effective_depth_compare(s: &PipelineState) -> CompareFunction {
    s.depth_compare.unwrap_or(CompareFunction::Always)
}

// --- stencil (u32 per face) -------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StencilFace {
    pub face: StencilFaceState,
    pub reference: u8,
    pub read_mask: u8,
}

fn stencil_op(code: u32) -> StencilOperation {
    match code {
        0 => StencilOperation::Zero,
        2 => StencilOperation::Replace,
        3 => StencilOperation::IncrementWrap,
        4 => StencilOperation::IncrementClamp,
        5 => StencilOperation::DecrementWrap,
        6 => StencilOperation::DecrementClamp,
        7 => StencilOperation::Invert,
        _ => StencilOperation::Keep,
    }
}

/// Decode one face's stencil word; `None` for `BGFX_STENCIL_NONE` (stencil off).
pub fn decode_stencil(word: u32) -> Option<StencilFace> {
    let test = compare(u64::from((word >> 16) & 0xF))?;
    Some(StencilFace {
        face: StencilFaceState {
            compare: test,
            fail_op: stencil_op((word >> 20) & 0xF),
            depth_fail_op: stencil_op((word >> 24) & 0xF),
            pass_op: stencil_op((word >> 28) & 0xF),
        },
        reference: (word & 0xFF) as u8,
        read_mask: ((word >> 8) & 0xFF) as u8,
    })
}

// --- clear ------------------------------------------------------------------------------------

pub const CLEAR_COLOR: u16 = 0x1;
pub const CLEAR_DEPTH: u16 = 0x2;
pub const CLEAR_STENCIL: u16 = 0x4;

/// bgfx packs clear colours as `0xRRGGBBAA`.
pub fn clear_color(rgba: u32) -> wgpu::Color {
    let c = |shift: u32| f64::from((rgba >> shift) & 0xFF) / 255.0;
    wgpu::Color { r: c(24), g: c(16), b: c(8), a: c(0) }
}

// --- samplers (u32) ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SamplerKey {
    pub address: [AddressMode; 3],
    pub mag: FilterMode,
    pub min: FilterMode,
    pub mip: MipmapFilterMode,
    pub compare: Option<CompareFunction>,
    pub anisotropy: u16,
}

impl SamplerKey {
    pub fn descriptor(&self) -> wgpu::SamplerDescriptor<'static> {
        wgpu::SamplerDescriptor {
            label: None,
            address_mode_u: self.address[0],
            address_mode_v: self.address[1],
            address_mode_w: self.address[2],
            mag_filter: self.mag,
            min_filter: self.min,
            mipmap_filter: self.mip,
            compare: self.compare,
            anisotropy_clamp: self.anisotropy,
            ..Default::default()
        }
    }
}

/// The highest anisotropy bgfx requests when a sampler asks for anisotropic filtering.
pub const MAX_ANISOTROPY: u16 = 16;

/// Decode bgfx sampler flags. `border_supported` says whether the device has
/// `ADDRESS_MODE_CLAMP_TO_BORDER`; without it, border addressing falls back to clamp.
pub fn decode_sampler(flags: u32, border_supported: bool) -> SamplerKey {
    let address = |shift: u32| match (flags >> shift) & 0x3 {
        1 => AddressMode::MirrorRepeat,
        2 => AddressMode::ClampToEdge,
        3 if border_supported => AddressMode::ClampToBorder,
        3 => AddressMode::ClampToEdge,
        _ => AddressMode::Repeat,
    };
    let min_bits = (flags >> 6) & 0x3;
    let mag_bits = (flags >> 8) & 0x3;
    let anisotropic = min_bits == 2 || mag_bits == 2;
    let filter = |bits: u32| if bits == 1 { FilterMode::Nearest } else { FilterMode::Linear };
    let mip = if (flags >> 10) & 0x1 == 1 { MipmapFilterMode::Nearest } else { MipmapFilterMode::Linear };
    if anisotropic {
        // wgpu only allows anisotropy with every filter linear, which is what D3D's
        // anisotropic filter does anyway.
        return SamplerKey {
            address: [address(0), address(2), address(4)],
            mag: FilterMode::Linear,
            min: FilterMode::Linear,
            mip: MipmapFilterMode::Linear,
            compare: compare(u64::from((flags >> 16) & 0xF)),
            anisotropy: MAX_ANISOTROPY,
        };
    }
    SamplerKey {
        address: [address(0), address(2), address(4)],
        mag: filter(mag_bits),
        min: filter(min_bits),
        mip,
        compare: compare(u64::from((flags >> 16) & 0xF)),
        anisotropy: 1,
    }
}

// --- texture formats --------------------------------------------------------------------------

/// `bgfx::TextureFormat::Enum` values the engine uses (the enum's declaration order).
pub mod bgfx_format {
    pub const BC1: u32 = 0;
    pub const BC2: u32 = 1;
    pub const BC3: u32 = 2;
    pub const BC4: u32 = 3;
    pub const BC5: u32 = 4;
    pub const UNKNOWN: u32 = 38;
    pub const A8: u32 = 40;
    pub const R8: u32 = 41;
    pub const R32F: u32 = 52;
    pub const RG16F: u32 = 60;
    pub const BGRA8: u32 = 70;
    pub const RGBA8: u32 = 71;
    pub const RGBA16F: u32 = 78;
    pub const RGBA32F: u32 = 82;
    pub const R5G6B5: u32 = 84;
    pub const BGRA4: u32 = 85;
    pub const BGR5A1: u32 = 87;
    pub const UNKNOWN_DEPTH: u32 = 91;
    pub const D16: u32 = 92;
    pub const D24: u32 = 93;
    pub const D24S8: u32 = 94;
    pub const D32: u32 = 95;
    pub const D32F: u32 = 98;
}

/// How texel data arriving in a bgfx format becomes texel data for wgpu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conversion {
    /// The bytes are uploaded as they are.
    None,
    /// D3D `A8`: one alpha byte, sampled as `(0, 0, 0, a)`, expanded to RGBA8.
    A8ToRgba8,
    /// D3D `R5G6B5` (`r << 11 | g << 5 | b`), expanded to BGRA8 with opaque alpha.
    R5g6b5ToBgra8,
    /// D3D `A1R5G5B5` (`a << 15 | r << 10 | g << 5 | b`), expanded to BGRA8.
    Bgr5a1ToBgra8,
    /// D3D `A4R4G4B4` (`a << 12 | r << 8 | g << 4 | b`), expanded to BGRA8.
    Bgra4ToBgra8,
    /// BC1/BC2/BC3 (DXT1/3/5) decoded to RGBA8 on the CPU, for GPUs without BC sampling.
    Bc1ToRgba8,
    Bc2ToRgba8,
    Bc3ToRgba8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormatPlan {
    pub format: TextureFormat,
    pub conversion: Conversion,
}

impl FormatPlan {
    /// Bytes per texel of the *source* data (0 for block-compressed formats).
    pub fn source_bytes_per_texel(&self) -> u32 {
        match self.conversion {
            Conversion::A8ToRgba8 => 1,
            Conversion::R5g6b5ToBgra8 | Conversion::Bgr5a1ToBgra8 | Conversion::Bgra4ToBgra8 => 2,
            Conversion::Bc1ToRgba8 | Conversion::Bc2ToRgba8 | Conversion::Bc3ToRgba8 => 0,
            Conversion::None => self.format.block_copy_size(None).filter(|_| self.format.block_dimensions() == (1, 1)).unwrap_or(0),
        }
    }
}

impl FormatPlan {
    /// For block-compressed source data: (block dimension, bytes per block).
    pub fn source_block(&self) -> Option<(u32, u32)> {
        match self.conversion {
            Conversion::Bc1ToRgba8 => Some((4, 8)),
            Conversion::Bc2ToRgba8 | Conversion::Bc3ToRgba8 => Some((4, 16)),
            Conversion::None if self.format.block_dimensions() != (1, 1) => {
                Some((self.format.block_dimensions().0, self.format.block_copy_size(None).unwrap_or(16)))
            }
            _ => None,
        }
    }
}

/// The plan on a GPU without BC sampling: BC1-3 decode to RGBA8 (the game does not use BC4/5).
pub fn without_bc(plan: FormatPlan) -> FormatPlan {
    let decode = |conversion| FormatPlan { format: TextureFormat::Rgba8Unorm, conversion };
    match plan.format {
        TextureFormat::Bc1RgbaUnorm => decode(Conversion::Bc1ToRgba8),
        TextureFormat::Bc2RgbaUnorm => decode(Conversion::Bc2ToRgba8),
        TextureFormat::Bc3RgbaUnorm => decode(Conversion::Bc3ToRgba8),
        _ => plan,
    }
}

fn rgb565(c: u16) -> [u8; 3] {
    let (r, g, b) = (c >> 11, (c >> 5) & 0x3F, c & 0x1F);
    [((r << 3) | (r >> 2)) as u8, ((g << 2) | (g >> 4)) as u8, ((b << 3) | (b >> 2)) as u8]
}

/// Decode one BC1-style colour block into 16 RGBA texels. `four_colour` forces the 4-colour
/// palette, as inside BC2/BC3 blocks.
fn decode_colour_block(b: &[u8], four_colour: bool, out: &mut [[u8; 4]; 16]) {
    let c0 = u16::from_le_bytes([b[0], b[1]]);
    let c1 = u16::from_le_bytes([b[2], b[3]]);
    let (p0, p1) = (rgb565(c0), rgb565(c1));
    let mix = |wa: u16, wb: u16| -> [u8; 4] {
        let m = |a: u8, b: u8| ((a as u16 * wa + b as u16 * wb) / (wa + wb)) as u8;
        [m(p0[0], p1[0]), m(p0[1], p1[1]), m(p0[2], p1[2]), 255]
    };
    let mut pal = [[p0[0], p0[1], p0[2], 255], [p1[0], p1[1], p1[2], 255], [0; 4], [0; 4]];
    if four_colour || c0 > c1 {
        pal[2] = mix(2, 1);
        pal[3] = mix(1, 2);
    } else {
        pal[2] = mix(1, 1);
        pal[3] = [0, 0, 0, 0];
    }
    let bits = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    for (i, t) in out.iter_mut().enumerate() {
        *t = pal[((bits >> (2 * i)) & 3) as usize];
    }
}

/// Decode `bw` x `bh` blocks of BC1/2/3 data (block rows `pitch` bytes apart) into RGBA8 rows of
/// `bw * 4` texels.
pub fn decode_bc(conversion: Conversion, data: &[u8], pitch: u32, bw: u32, bh: u32) -> Vec<u8> {
    let block_bytes = if conversion == Conversion::Bc1ToRgba8 { 8 } else { 16 };
    let row_texels = (bw * 4) as usize;
    let mut out = vec![0u8; row_texels * 4 * (bh * 4) as usize];
    let mut texels = [[0u8; 4]; 16];
    for by in 0..bh as usize {
        for bx in 0..bw as usize {
            let at = by * pitch as usize + bx * block_bytes;
            let Some(b) = data.get(at..at + block_bytes) else { continue };
            match conversion {
                Conversion::Bc1ToRgba8 => decode_colour_block(b, false, &mut texels),
                Conversion::Bc2ToRgba8 => {
                    decode_colour_block(&b[8..], true, &mut texels);
                    for (i, t) in texels.iter_mut().enumerate() {
                        t[3] = ((b[i / 2] >> (4 * (i % 2))) & 0xF) * 17;
                    }
                }
                _ => {
                    decode_colour_block(&b[8..], true, &mut texels);
                    let (a0, a1) = (b[0] as u16, b[1] as u16);
                    let mut alpha = [a0, a1, 0, 0, 0, 0, 0, 0];
                    if a0 > a1 {
                        for i in 1..7u16 {
                            alpha[i as usize + 1] = ((7 - i) * a0 + i * a1) / 7;
                        }
                    } else {
                        for i in 1..5u16 {
                            alpha[i as usize + 1] = ((5 - i) * a0 + i * a1) / 5;
                        }
                        alpha[7] = 255;
                    }
                    let bits = u64::from_le_bytes([b[2], b[3], b[4], b[5], b[6], b[7], 0, 0]);
                    for (i, t) in texels.iter_mut().enumerate() {
                        t[3] = alpha[((bits >> (3 * i)) & 7) as usize] as u8;
                    }
                }
            }
            for (i, t) in texels.iter().enumerate() {
                let (x, y) = (bx * 4 + i % 4, by * 4 + i / 4);
                let o = (y * row_texels + x) * 4;
                out[o..o + 4].copy_from_slice(t);
            }
        }
    }
    out
}

/// The wgpu format and upload conversion for a bgfx format, or `None` if unsupported.
pub fn map_format(bgfx: u32) -> Option<FormatPlan> {
    use bgfx_format as f;
    let plain = |format| Some(FormatPlan { format, conversion: Conversion::None });
    let converted = |format, conversion| Some(FormatPlan { format, conversion });
    match bgfx {
        f::BC1 => plain(TextureFormat::Bc1RgbaUnorm),
        f::BC2 => plain(TextureFormat::Bc2RgbaUnorm),
        f::BC3 => plain(TextureFormat::Bc3RgbaUnorm),
        f::BC4 => plain(TextureFormat::Bc4RUnorm),
        f::BC5 => plain(TextureFormat::Bc5RgUnorm),
        f::A8 => converted(TextureFormat::Rgba8Unorm, Conversion::A8ToRgba8),
        f::R8 => plain(TextureFormat::R8Unorm),
        f::R32F => plain(TextureFormat::R32Float),
        f::RG16F => plain(TextureFormat::Rg16Float),
        f::BGRA8 => plain(TextureFormat::Bgra8Unorm),
        f::RGBA8 => plain(TextureFormat::Rgba8Unorm),
        f::RGBA16F => plain(TextureFormat::Rgba16Float),
        f::RGBA32F => plain(TextureFormat::Rgba32Float),
        f::R5G6B5 => converted(TextureFormat::Bgra8Unorm, Conversion::R5g6b5ToBgra8),
        f::BGRA4 => converted(TextureFormat::Bgra8Unorm, Conversion::Bgra4ToBgra8),
        f::BGR5A1 => converted(TextureFormat::Bgra8Unorm, Conversion::Bgr5a1ToBgra8),
        f::D16 => plain(TextureFormat::Depth16Unorm),
        f::D24 | f::UNKNOWN_DEPTH => plain(TextureFormat::Depth24Plus),
        f::D24S8 => plain(TextureFormat::Depth24PlusStencil8),
        f::D32 | f::D32F => plain(TextureFormat::Depth32Float),
        _ => None,
    }
}

/// Expand `src` (tightly packed source texels) into the plan's wgpu format.
pub fn convert_texels(conversion: Conversion, src: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    use std::borrow::Cow;
    let expand5 = |v: u16| ((v << 3) | (v >> 2)) as u8;
    let expand6 = |v: u16| ((v << 2) | (v >> 4)) as u8;
    let expand4 = |v: u16| (v * 17) as u8;
    let words = || src.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]]));
    match conversion {
        Conversion::None | Conversion::Bc1ToRgba8 | Conversion::Bc2ToRgba8 | Conversion::Bc3ToRgba8 => Cow::Borrowed(src),
        Conversion::A8ToRgba8 => Cow::Owned(src.iter().flat_map(|&a| [0, 0, 0, a]).collect()),
        Conversion::R5g6b5ToBgra8 => Cow::Owned(
            words().flat_map(|w| [expand5(w & 0x1F), expand6((w >> 5) & 0x3F), expand5(w >> 11), 0xFF]).collect(),
        ),
        Conversion::Bgr5a1ToBgra8 => Cow::Owned(
            words()
                .flat_map(|w| {
                    [expand5(w & 0x1F), expand5((w >> 5) & 0x1F), expand5((w >> 10) & 0x1F), if w >> 15 != 0 { 0xFF } else { 0 }]
                })
                .collect(),
        ),
        Conversion::Bgra4ToBgra8 => Cow::Owned(
            words()
                .flat_map(|w| [expand4(w & 0xF), expand4((w >> 4) & 0xF), expand4((w >> 8) & 0xF), expand4(w >> 12)])
                .collect(),
        ),
    }
}

// --- vertex layouts ---------------------------------------------------------------------------

/// `bgfx::Attrib::Enum` order.
pub const ATTRIB_SEMANTICS: [&str; 18] = [
    "POSITION", "NORMAL", "TANGENT", "BITANGENT", "COLOR0", "COLOR1", "COLOR2", "COLOR3", "INDICES", "WEIGHT",
    "TEXCOORD0", "TEXCOORD1", "TEXCOORD2", "TEXCOORD3", "TEXCOORD4", "TEXCOORD5", "TEXCOORD6", "TEXCOORD7",
];

/// The wgpu vertex format for a bgfx attribute (`bgfx::AttribType::Enum`: Int8, Uint8, Uint10,
/// Int16, Uint16, Half, Float) with `num` components.
pub fn vertex_format(attrib_type: u8, num: u8, normalized: bool, as_int: bool) -> Option<wgpu::VertexFormat> {
    use wgpu::VertexFormat as V;
    Some(match (attrib_type, num, normalized, as_int) {
        // Uint8 is only addressable in groups of 2 or 4; bgfx's 3-component colours are padded.
        (1, 3 | 4, true, _) => V::Unorm8x4,
        // Unnormalised integers read as floats have no wgpu format; they are unsupported.
        (1, 4, false, true) => V::Uint8x4,
        (1, 2, true, _) => V::Unorm8x2,
        (1, 2, false, true) => V::Uint8x2,
        (0, 4, true, _) => V::Snorm8x4,
        (0, 4, false, true) => V::Sint8x4,
        (3, 2, true, _) => V::Snorm16x2,
        (3, 4, true, _) => V::Snorm16x4,
        (3, 2, false, true) => V::Sint16x2,
        (3, 4, false, true) => V::Sint16x4,
        (4, 2, true, _) => V::Unorm16x2,
        (4, 4, true, _) => V::Unorm16x4,
        (5, 2, _, _) => V::Float16x2,
        (5, 4, _, _) => V::Float16x4,
        (6, 1, _, _) => V::Float32,
        (6, 2, _, _) => V::Float32x2,
        (6, 3, _, _) => V::Float32x3,
        (6, 4, _, _) => V::Float32x4,
        _ => return None,
    })
}

/// Bytes one bgfx attribute occupies in a vertex (bgfx's `s_attribTypeSize` table).
pub fn attrib_size(attrib_type: u8, num: u8) -> u16 {
    let num = u16::from(num.clamp(1, 4));
    match attrib_type {
        0 | 1 => [1, 2, 4, 4][usize::from(num - 1)],
        2 => 4,
        3 | 4 => if num <= 2 { 2 * num } else { 8 },
        5 => if num <= 2 { 2 * num } else { 8 },
        _ => 4 * num,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Composite values as the BGFX_STATE_* macros build them.
    const DEFAULT: u64 = 0xF | STATE_WRITE_Z | (2 << 4) | (1 << 36) | STATE_MSAA; // RGB|A|Z|LESS-ish|CULL_CW|MSAA
    const BLEND_ALPHA: u64 = (5 << 12) | (6 << 16) | (5 << 20) | (6 << 24); // SRC_ALPHA, INV_SRC_ALPHA

    #[test]
    fn default_state() {
        let s = decode_state(DEFAULT);
        assert_eq!(s.write_mask, ColorWrites::ALL);
        assert!(s.depth_write);
        assert_eq!(s.depth_compare, Some(CompareFunction::LessEqual));
        assert_eq!(s.blend, None);
        assert_eq!(s.cull_mode, Some(Face::Back));
        assert_eq!(s.topology, PrimitiveTopology::TriangleList);
        assert!(s.msaa);
    }

    #[test]
    fn alpha_blend_and_alpha_ref() {
        let s = decode_state(STATE_WRITE_R | STATE_WRITE_G | STATE_WRITE_B | BLEND_ALPHA | (0x60 << 40));
        let b = s.blend.unwrap();
        assert_eq!(b.color, BlendComponent { src_factor: BlendFactor::SrcAlpha, dst_factor: BlendFactor::OneMinusSrcAlpha, operation: BlendOperation::Add });
        assert_eq!(b.alpha, b.color);
        assert!(!s.write_mask.contains(ColorWrites::ALPHA));
        assert!(!s.depth_write);
        assert_eq!(s.depth_compare, None);
        assert_eq!(alpha_ref(0x60 << 40), 0x60);
    }

    #[test]
    fn blend_equations_and_separate_alpha() {
        // additive colour, reverse-subtract alpha (equation alpha in bits 31..33).
        let state = (2 << 12) | (2 << 16) | (1 << 20) | (2 << 24) | (2u64 << 31);
        let b = decode_state(state).blend.unwrap();
        assert_eq!(b.color.operation, BlendOperation::Add);
        assert_eq!((b.color.src_factor, b.color.dst_factor), (BlendFactor::One, BlendFactor::One));
        assert_eq!(b.alpha.operation, BlendOperation::ReverseSubtract);
        assert_eq!(b.alpha.src_factor, BlendFactor::Zero);
        // MAX forces One/One.
        let b = decode_state((5 << 12) | (6 << 16) | (4u64 << 28)).blend.unwrap();
        assert_eq!(b.color, BlendComponent { src_factor: BlendFactor::One, dst_factor: BlendFactor::One, operation: BlendOperation::Max });
    }

    #[test]
    fn cull_and_primitives() {
        assert_eq!(decode_state(2 << 36).cull_mode, Some(Face::Front));
        assert_eq!(decode_state(0).cull_mode, None);
        assert_eq!(decode_state(1 << 48).topology, PrimitiveTopology::TriangleStrip);
        assert_eq!(decode_state(2 << 48).topology, PrimitiveTopology::LineList);
        assert_eq!(decode_state(3 << 48).topology, PrimitiveTopology::LineStrip);
        assert_eq!(decode_state(4 << 48).topology, PrimitiveTopology::PointList);
    }

    #[test]
    fn stencil_words() {
        assert_eq!(decode_stencil(0), None);
        // REF(3) | RMASK(0xFF) | TEST_EQUAL | OP_FAIL_S_KEEP | OP_FAIL_Z_INCR | OP_PASS_Z_REPLACE
        let w = 3 | (0xFF << 8) | (3 << 16) | (1 << 20) | (3 << 24) | (2 << 28);
        let s = decode_stencil(w).unwrap();
        assert_eq!(s.reference, 3);
        assert_eq!(s.read_mask, 0xFF);
        assert_eq!(s.face.compare, CompareFunction::Equal);
        assert_eq!(s.face.fail_op, StencilOperation::Keep);
        assert_eq!(s.face.depth_fail_op, StencilOperation::IncrementWrap);
        assert_eq!(s.face.pass_op, StencilOperation::Replace);
    }

    #[test]
    fn samplers() {
        let s = decode_sampler(0, true);
        assert_eq!(s.address, [AddressMode::Repeat; 3]);
        assert_eq!((s.min, s.mag, s.mip), (FilterMode::Linear, FilterMode::Linear, MipmapFilterMode::Linear));
        // U_CLAMP | V_CLAMP | POINT
        let s = decode_sampler(0x2 | 0x8 | 0x40 | 0x100 | 0x400, true);
        assert_eq!(s.address, [AddressMode::ClampToEdge, AddressMode::ClampToEdge, AddressMode::Repeat]);
        assert_eq!((s.min, s.mag, s.mip), (FilterMode::Nearest, FilterMode::Nearest, MipmapFilterMode::Nearest));
        // MIN_ANISOTROPIC | MAG_ANISOTROPIC | MIP_POINT: anisotropy forces linear everywhere.
        let s = decode_sampler(0x80 | 0x200 | 0x400, true);
        assert_eq!(s.anisotropy, MAX_ANISOTROPY);
        assert_eq!(s.mip, MipmapFilterMode::Linear);
        // border falls back to clamp when unsupported; comparison sampler.
        let s = decode_sampler(0x3 | 0x00020000, false);
        assert_eq!(s.address[0], AddressMode::ClampToEdge);
        assert_eq!(s.compare, Some(CompareFunction::LessEqual));
    }

    #[test]
    fn clear_colour_is_rgba() {
        let c = clear_color(0xFF000080);
        assert_eq!((c.r, c.g, c.b), (1.0, 0.0, 0.0));
        assert!((c.a - 128.0 / 255.0).abs() < 1e-9);
    }

    #[test]
    fn formats_and_conversions() {
        assert_eq!(map_format(bgfx_format::BC3).unwrap().format, TextureFormat::Bc3RgbaUnorm);
        assert_eq!(map_format(bgfx_format::BGRA8).unwrap().source_bytes_per_texel(), 4);
        assert_eq!(map_format(bgfx_format::BC1).unwrap().source_bytes_per_texel(), 0);
        assert_eq!(map_format(bgfx_format::R5G6B5).unwrap().source_bytes_per_texel(), 2);
        assert_eq!(map_format(bgfx_format::UNKNOWN), None);

        // pure red in each packed format → BGRA8 (B, G, R, A)
        let red565 = (0x1Fu16 << 11).to_le_bytes();
        assert_eq!(&*convert_texels(Conversion::R5g6b5ToBgra8, &red565), &[0, 0, 255, 255]);
        let red1555 = (0x8000u16 | (0x1F << 10)).to_le_bytes();
        assert_eq!(&*convert_texels(Conversion::Bgr5a1ToBgra8, &red1555), &[0, 0, 255, 255]);
        let half_red4444 = (0x8F00u16).to_le_bytes();
        assert_eq!(&*convert_texels(Conversion::Bgra4ToBgra8, &half_red4444), &[0, 0, 255, 0x88]);
        assert_eq!(&*convert_texels(Conversion::A8ToRgba8, &[7]), &[0, 0, 0, 7]);
    }

    #[test]
    fn bc_decoding() {
        // BC1: solid green, then 3-colour mode (c0 <= c1) where index 3 is transparent.
        let green = decode_bc(Conversion::Bc1ToRgba8, &[0xE0, 0x07, 0xE0, 0x07, 0, 0, 0, 0], 8, 1, 1);
        assert_eq!(&green[..4], &[0, 255, 0, 255]);
        let clear = decode_bc(Conversion::Bc1ToRgba8, &[0, 0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF], 8, 1, 1);
        assert_eq!(clear[3], 0);
        // BC3: alpha endpoints 255 and 0, index 1 (= a1 = 0) everywhere, over red.
        let b = [255u8, 0, 0x49, 0x92, 0x24, 0x49, 0x92, 0x24, 0x00, 0xF8, 0x00, 0xF8, 0, 0, 0, 0];
        assert_eq!(&decode_bc(Conversion::Bc3ToRgba8, &b, 16, 1, 1)[..4], &[255, 0, 0, 0]);
        // BC2: explicit 4-bit alpha 8 on texel 0.
        let b = [0x08u8, 0, 0, 0, 0, 0, 0, 0, 0x00, 0xF8, 0x00, 0xF8, 0, 0, 0, 0];
        assert_eq!(&decode_bc(Conversion::Bc2ToRgba8, &b, 16, 1, 1)[..4], &[255, 0, 0, 136]);
        assert_eq!(without_bc(map_format(bgfx_format::BC1).unwrap()).source_block(), Some((4, 8)));
    }

    #[test]
    fn vertex_formats() {
        assert_eq!(vertex_format(6, 3, false, false), Some(wgpu::VertexFormat::Float32x3));
        assert_eq!(vertex_format(1, 4, true, false), Some(wgpu::VertexFormat::Unorm8x4));
        assert_eq!(attrib_size(6, 3), 12);
        assert_eq!(attrib_size(1, 4), 4);
        assert_eq!(attrib_size(1, 1), 1);
        assert_eq!(vertex_format(1, 4, false, false), None);
        assert_eq!(ATTRIB_SEMANTICS[10], "TEXCOORD0");
    }
}
