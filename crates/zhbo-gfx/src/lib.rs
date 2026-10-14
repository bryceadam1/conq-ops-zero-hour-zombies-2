//! zhbo-gfx: the Conquer Ops renderer.
//!
//! The engine's render backend (bobtista's IRenderBackend over WW3D2) was written against a subset of
//! the bgfx API. This crate implements that subset on wgpu, so the same backend runs on Vulkan, Metal
//! and Direct3D 12 without the bgfx library. See docs/RENDERER.md.

pub mod shader;
pub mod state;
