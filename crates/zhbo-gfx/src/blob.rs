//! The compiled-shader blob: what `zhbo-shaderc` writes at build time in place of shaderc's
//! bytecode, and what `bgfx::createShader` hands the runtime.
//!
//! Layout (little-endian): magic `ZHS1`, stage (u8: 0 vertex, 1 fragment), uniform block size
//! (u32), uniforms (u16 count; each name, kind u8, count u16, offset u32), samplers (u8 count; each
//! name, slot u8, array u8), attributes (u8 count; each name, semantic, location u8), WGSL source
//! (u32 length + bytes). Names are u8-length-prefixed.

use crate::shader::{self, AttributeInfo, SamplerInfo, Stage, Translated, UniformInfo, UniformKind, VaryingDef};

pub const MAGIC: &[u8; 4] = b"ZHS1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShaderBlob {
    pub stage: Stage,
    pub uniform_block_size: u32,
    pub uniforms: Vec<UniformInfo>,
    pub samplers: Vec<SamplerInfo>,
    pub attributes: Vec<AttributeInfo>,
    pub wgsl: String,
}

/// Translate a bgfx `.sc` shader and generate its WGSL: the whole build-time step.
pub fn build(src: &str, varyings: &[VaryingDef], stage: Stage, defines: &[(&str, &str)]) -> Result<ShaderBlob, String> {
    let t: Translated = shader::translate(src, varyings, stage, defines)?;
    let (module, info) = shader::compile(&t)?;
    let wgsl = naga::back::wgsl::write_string(&module, &info, naga::back::wgsl::WriterFlags::empty())
        .map_err(|e| format!("WGSL generation failed: {e}"))?;
    Ok(ShaderBlob {
        stage,
        uniform_block_size: t.uniform_block_size,
        uniforms: t.uniforms,
        samplers: t.samplers,
        attributes: t.attributes,
        wgsl,
    })
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    let b = s.as_bytes();
    out.push(b.len().min(255) as u8);
    out.extend_from_slice(&b[..b.len().min(255)]);
}

impl ShaderBlob {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.wgsl.len() + 256);
        out.extend_from_slice(MAGIC);
        out.push(match self.stage {
            Stage::Vertex => 0,
            Stage::Fragment => 1,
        });
        out.extend_from_slice(&self.uniform_block_size.to_le_bytes());
        out.extend_from_slice(&(self.uniforms.len() as u16).to_le_bytes());
        for u in &self.uniforms {
            put_str(&mut out, &u.name);
            out.push(match u.kind {
                UniformKind::Vec4 => 0,
                UniformKind::Mat4 => 1,
            });
            out.extend_from_slice(&(u.count as u16).to_le_bytes());
            out.extend_from_slice(&u.offset.to_le_bytes());
        }
        out.push(self.samplers.len() as u8);
        for s in &self.samplers {
            put_str(&mut out, &s.name);
            out.push(s.slot as u8);
            out.push(s.array as u8);
        }
        out.push(self.attributes.len() as u8);
        for a in &self.attributes {
            put_str(&mut out, &a.name);
            put_str(&mut out, &a.semantic);
            out.push(a.location as u8);
        }
        out.extend_from_slice(&(self.wgsl.len() as u32).to_le_bytes());
        out.extend_from_slice(self.wgsl.as_bytes());
        out
    }

    pub fn decode(data: &[u8]) -> Result<ShaderBlob, String> {
        let mut r = Reader { data, pos: 0 };
        if r.take(4)? != MAGIC {
            return Err("not a zhbo shader blob (was the shader compiled by bgfx's shaderc?)".into());
        }
        let stage = match r.u8()? {
            0 => Stage::Vertex,
            1 => Stage::Fragment,
            s => return Err(format!("bad stage {s}")),
        };
        let uniform_block_size = r.u32()?;
        let mut uniforms = Vec::new();
        for _ in 0..r.u16()? {
            let name = r.str()?;
            let kind = if r.u8()? == 1 { UniformKind::Mat4 } else { UniformKind::Vec4 };
            let count = u32::from(r.u16()?);
            let offset = r.u32()?;
            uniforms.push(UniformInfo { name, kind, count, offset });
        }
        let mut samplers = Vec::new();
        for _ in 0..r.u8()? {
            let name = r.str()?;
            let slot = u32::from(r.u8()?);
            let array = r.u8()? != 0;
            samplers.push(SamplerInfo { name, slot, array });
        }
        let mut attributes = Vec::new();
        for _ in 0..r.u8()? {
            let name = r.str()?;
            let semantic = r.str()?;
            let location = u32::from(r.u8()?);
            attributes.push(AttributeInfo { name, semantic, location });
        }
        let len = r.u32()? as usize;
        let wgsl = String::from_utf8(r.take(len)?.to_vec()).map_err(|_| "WGSL is not UTF-8")?;
        Ok(ShaderBlob { stage, uniform_block_size, uniforms, samplers, attributes, wgsl })
    }
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.data.len()).ok_or("truncated shader blob")?;
        let s = &self.data[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn str(&mut self) -> Result<String, String> {
        let n = self.u8()? as usize;
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| "name is not UTF-8".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let varyings = shader::parse_varying_def(
            "vec4 v_color0 : COLOR0;\nvec3 a_position : POSITION;\nvec4 a_color0 : COLOR0;\n",
        );
        let src = "$input a_position, a_color0\n$output v_color0\n#include <bgfx_shader.sh>\nuniform vec4 u_tint[3];\nvoid main() { gl_Position = mul(u_modelViewProj, vec4(a_position, 1.0)); v_color0 = a_color0 * u_tint[2]; }\n";
        let blob = build(src, &varyings, Stage::Vertex, &[]).unwrap();
        assert!(blob.wgsl.contains("@vertex"));
        let back = ShaderBlob::decode(&blob.encode()).unwrap();
        assert_eq!(back, blob);
        assert!(ShaderBlob::decode(b"VSH\x05garbage").is_err());
    }
}
