// Conquer Ops: the bgfx API, as the engine's BgfxBackend uses it, implemented on the Rust wgpu
// renderer (crates/zhbo-gfx). Built instead of the bgfx library when GGC_BGFX_RENDERER=wgpu.
//
// Declarations come from bgfx's own bgfx.h (BSD-2-Clause, pinned by the engine's bgfx.cmake), so
// the backend compiles unchanged. Only the functions the backend calls are defined; the linker
// reports any new one. Memory blocks are consumed (and released) by the call they are passed to,
// which is safe because the renderer copies or uploads their contents immediately.

#include <bgfx/bgfx.h>

#include <cstdarg>
#include <cstdio>
#include <cstdlib>
#include <cstring>

#if defined(__linux__) && defined(SAGE_USE_SDL3)
#include <SDL3/SDL_video.h>
#endif

#include "zhbo_gfx.h"

namespace
{
	struct MemoryBlock
	{
		bgfx::Memory mem; // first, so a Memory* is a MemoryBlock*
		bgfx::ReleaseFn release;
		void *userData;
	};

	bgfx::CallbackI *g_callback = nullptr;
	bgfx::PlatformData g_platformData;
	bgfx::Caps g_caps;
	bgfx::Stats g_stats;
	uint32_t g_width = 0;
	uint32_t g_height = 0;
	char g_rendererName[128] = "WebGPU (wgpu)";

	// createUniform type and size, for setUniform's element counts.
	struct UniformInfo
	{
		uint8_t floatsPerElement;
		uint16_t num;
	};
	UniformInfo g_uniforms[UINT16_MAX];

	void Release(const bgfx::Memory *mem)
	{
		if (mem == nullptr)
		{
			return;
		}
		MemoryBlock *block = (MemoryBlock *)mem;
		if (block->release != nullptr)
		{
			block->release(block->mem.data, block->userData);
		}
		std::free(block);
	}

	ZhgfxLayout ToLayout(const bgfx::VertexLayout &layout)
	{
		ZhgfxLayout out;
		out.stride = layout.m_stride;
		for (int i = 0; i < 18; ++i)
		{
			out.offset[i] = layout.m_offset[i];
			out.attributes[i] = layout.m_attributes[i];
		}
		return out;
	}

	void TraceVa(const char *format, ...)
	{
		va_list args;
		va_start(args, format);
		if (g_callback != nullptr)
		{
			g_callback->traceVargs(__FILE__, uint16_t(__LINE__), format, args);
		}
		else
		{
			std::vfprintf(stderr, format, args);
		}
		va_end(args);
	}

	void Trace(const char *msg)
	{
		TraceVa("%s\n", msg);
	}

	void Screenshot(const char *path, uint32_t width, uint32_t height, uint32_t pitch, const uint8_t *data, uint32_t size)
	{
		if (g_callback != nullptr)
		{
			g_callback->screenShot(path, width, height, pitch, bgfx::TextureFormat::BGRA8, data, size, false);
		}
	}

	uint16_t RatioSize(uint32_t size, bgfx::BackbufferRatio::Enum ratio)
	{
		switch (ratio)
		{
			case bgfx::BackbufferRatio::Half:      size /= 2; break;
			case bgfx::BackbufferRatio::Quarter:   size /= 4; break;
			case bgfx::BackbufferRatio::Eighth:    size /= 8; break;
			case bgfx::BackbufferRatio::Sixteenth: size /= 16; break;
			case bgfx::BackbufferRatio::Double:    size *= 2; break;
			default: break;
		}
		return uint16_t(size < 1 ? 1 : size);
	}

	void FillCaps()
	{
		std::memset(&g_caps, 0, sizeof(g_caps));
		g_caps.rendererType = bgfx::RendererType::WebGPU;
		g_caps.supported = BGFX_CAPS_INSTANCING | BGFX_CAPS_INDEX32 | BGFX_CAPS_TEXTURE_BLIT
			| BGFX_CAPS_TEXTURE_2D_ARRAY | BGFX_CAPS_ALPHA_TO_COVERAGE | BGFX_CAPS_VERTEX_ATTRIB_HALF
;
		g_caps.homogeneousDepth = false; // depth is 0..1, as in D3D, Metal and Vulkan
		g_caps.originBottomLeft = false;
		g_caps.numGPUs = 1;
		g_caps.limits.maxDrawCalls = 65535;
		g_caps.limits.maxBlits = 1024;
		g_caps.limits.maxTextureSize = zhgfx_max_texture_size();
		g_caps.limits.maxTextureLayers = 256;
		g_caps.limits.maxViews = 256;
		g_caps.limits.maxFrameBuffers = 1024;
		g_caps.limits.maxFBAttachments = 8;
		g_caps.limits.maxPrograms = 512;
		g_caps.limits.maxShaders = 512;
		g_caps.limits.maxTextures = 65534;
		g_caps.limits.maxTextureSamplers = 16;
		g_caps.limits.maxVertexLayouts = 1024;
		g_caps.limits.maxVertexStreams = 4;
		g_caps.limits.maxIndexBuffers = 65534;
		g_caps.limits.maxVertexBuffers = 65534;
		g_caps.limits.maxDynamicIndexBuffers = 65534;
		g_caps.limits.maxDynamicVertexBuffers = 65534;
		g_caps.limits.maxUniforms = 1024;
		g_caps.limits.maxEncoders = 1;
		g_caps.limits.maxTransientVbSize = 6 << 20;
		g_caps.limits.maxTransientIbSize = 2 << 20;
		for (uint32_t i = 0; i < bgfx::TextureFormat::Count; ++i)
		{
			g_caps.formats[i] = zhgfx_format_caps(i);
		}
	}

	uint32_t KindFor(const bgfx::PlatformData &pd)
	{
		if (pd.nwh == nullptr)
		{
			return 0;
		}
#if defined(_WIN32)
		return 1;
#elif defined(__APPLE__)
		return 2; // the backend passes SDL's CAMetalLayer
#else
#if defined(SAGE_USE_SDL3)
		const char *driver = SDL_GetCurrentVideoDriver();
		if (driver != nullptr && std::strcmp(driver, "wayland") == 0)
		{
			return 4;
		}
#endif
		return pd.type == bgfx::NativeWindowHandleType::Wayland ? 4 : 3;
#endif
	}
}

namespace bgfx
{
	// --- structs with out-of-line members ------------------------------------------------------

	CallbackI::~CallbackI() {}

	PlatformData::PlatformData()
		: ndt(nullptr), nwh(nullptr), context(nullptr), queue(nullptr), backBuffer(nullptr), backBufferDS(nullptr),
		  type(NativeWindowHandleType::Default)
	{
	}

	Resolution::Resolution()
		: formatColor(TextureFormat::RGBA8), formatDepthStencil(TextureFormat::D24S8), width(1280), height(720),
		  reset(BGFX_RESET_NONE), numBackBuffers(2), maxFrameLatency(0), debugTextScale(0)
	{
	}

	Init::Limits::Limits()
		: maxEncoders(8), minResourceCbSize(64 << 10), maxTransientVbSize(6 << 20), maxTransientIbSize(2 << 20),
		  minUniformBufferSize(1 << 20)
	{
	}

	Init::Init()
		: type(RendererType::Count), vendorId(BGFX_PCI_ID_NONE), deviceId(0), capabilities(UINT64_MAX), debug(false),
		  profile(false), fallback(true), callback(nullptr), allocator(nullptr)
	{
	}

	void Attachment::init(TextureHandle _handle, Access::Enum _access, uint16_t _layer, uint16_t _numLayers, uint16_t _mip, uint8_t _resolve)
	{
		access = _access;
		handle = _handle;
		mip = _mip;
		layer = _layer;
		numLayers = _numLayers;
		resolve = _resolve;
	}

	// bgfx's attribute sizes (the D3D/Noop table): [type][num - 1].
	static const uint8_t kAttribTypeSize[AttribType::Count][4] = {
		{ 4, 4, 4, 4 },   // Int8
		{ 1, 2, 4, 4 },   // Uint8
		{ 4, 4, 4, 4 },   // Uint10
		{ 2, 4, 8, 8 },   // Int16
		{ 2, 4, 8, 8 },   // Uint16
		{ 2, 4, 8, 8 },   // Half
		{ 4, 8, 12, 16 }, // Float
	};

	VertexLayout::VertexLayout() : m_stride(0) {}

	VertexLayout &VertexLayout::begin(RendererType::Enum _rendererType)
	{
		m_hash = uint32_t(_rendererType);
		m_stride = 0;
		std::memset(m_attributes, 0xff, sizeof(m_attributes));
		std::memset(m_offset, 0, sizeof(m_offset));
		return *this;
	}

	VertexLayout &VertexLayout::add(Attrib::Enum _attrib, uint8_t _num, AttribType::Enum _type, bool _normalized, bool _asInt)
	{
		const uint16_t encodedNorm = (_normalized & 1) << 7;
		const uint16_t encodedType = (_type & 7) << 3;
		const uint16_t encodedNum = (_num - 1) & 3;
		const uint16_t encodeAsInt = (_asInt & (_type < AttribType::Uint16)) << 8;
		m_attributes[_attrib] = encodedNorm | encodedType | encodedNum | encodeAsInt;
		m_offset[_attrib] = m_stride;
		m_stride += kAttribTypeSize[_type][(_num - 1) & 3];
		return *this;
	}

	VertexLayout &VertexLayout::skip(uint8_t _num)
	{
		m_stride += _num;
		return *this;
	}

	void VertexLayout::end()
	{
		// FNV-1a over the layout; only used as an identity.
		uint32_t hash = 2166136261u;
		const uint8_t *bytes[] = { (const uint8_t *)m_attributes, (const uint8_t *)m_offset, (const uint8_t *)&m_stride };
		const size_t sizes[] = { sizeof(m_attributes), sizeof(m_offset), sizeof(m_stride) };
		for (int part = 0; part < 3; ++part)
		{
			for (size_t i = 0; i < sizes[part]; ++i)
			{
				hash = (hash ^ bytes[part][i]) * 16777619u;
			}
		}
		m_hash = hash;
	}

	void VertexLayout::decode(Attrib::Enum _attrib, uint8_t &_num, AttribType::Enum &_type, bool &_normalized, bool &_asInt) const
	{
		const uint16_t val = m_attributes[_attrib];
		_num = (val & 3) + 1;
		_type = AttribType::Enum((val >> 3) & 7);
		_normalized = !!(val & (1 << 7));
		_asInt = !!(val & (1 << 8));
	}

	// --- lifetime --------------------------------------------------------------------------------

	void setPlatformData(const PlatformData &_data)
	{
		g_platformData = _data;
	}

	RenderFrame::Enum renderFrame(int32_t)
	{
		// The renderer is always single-threaded; the backend's pre-init call is a no-op.
		return RenderFrame::NoContext;
	}

	bool init(const Init &_init)
	{
		g_callback = _init.callback;
		zhgfx_set_callbacks(&Trace, &Screenshot);
		const PlatformData &pd = _init.platformData.nwh != nullptr ? _init.platformData : g_platformData;
		ZhgfxInit desc;
		desc.kind = KindFor(pd);
		desc.window = pd.nwh;
		desc.display = pd.ndt;
		desc.width = _init.resolution.width;
		desc.height = _init.resolution.height;
		desc.reset = _init.resolution.reset;
		desc.transient_vb_size = _init.limits.maxTransientVbSize;
		desc.transient_ib_size = _init.limits.maxTransientIbSize;
		if (!zhgfx_init(&desc))
		{
			return false;
		}
		g_width = desc.width;
		g_height = desc.height;
		std::memset(g_uniforms, 0, sizeof(g_uniforms));
		zhgfx_adapter_name(g_rendererName, sizeof(g_rendererName));
		FillCaps();
		return true;
	}

	void shutdown()
	{
		zhgfx_shutdown();
		g_callback = nullptr;
	}

	void reset(uint32_t _width, uint32_t _height, uint32_t _flags, TextureFormat::Enum)
	{
		g_width = _width;
		g_height = _height;
		zhgfx_reset(_width, _height, _flags);
	}

	uint32_t frame(uint8_t)
	{
		return zhgfx_frame();
	}

	RendererType::Enum getRendererType()
	{
		return RendererType::WebGPU;
	}

	const char *getRendererName(RendererType::Enum _type)
	{
		return _type == RendererType::WebGPU ? g_rendererName : "Unavailable (Conquer Ops uses wgpu)";
	}

	const Caps *getCaps()
	{
		return &g_caps;
	}

	const Stats *getStats()
	{
		ZhgfxStats s;
		zhgfx_stats(&s);
		std::memset(&g_stats, 0, sizeof(g_stats));
		g_stats.cpuTimerFreq = 1000000000;
		g_stats.gpuTimerFreq = 1000000000;
		g_stats.cpuTimeFrame = int64_t(s.cpu_frame_ns);
		g_stats.numDraw = s.num_draw;
		g_stats.numBlit = s.num_blit;
		g_stats.numTextures = uint16_t(s.num_textures);
		g_stats.numFrameBuffers = uint16_t(s.num_frame_buffers);
		g_stats.transientVbUsed = int32_t(s.transient_vb_used);
		g_stats.transientIbUsed = int32_t(s.transient_ib_used);
		g_stats.width = uint16_t(g_width);
		g_stats.height = uint16_t(g_height);
		return &g_stats;
	}

	// --- memory ----------------------------------------------------------------------------------

	const Memory *alloc(uint32_t _size)
	{
		MemoryBlock *block = (MemoryBlock *)std::malloc(sizeof(MemoryBlock) + _size);
		block->mem.data = (uint8_t *)(block + 1);
		block->mem.size = _size;
		block->release = nullptr;
		block->userData = nullptr;
		return &block->mem;
	}

	const Memory *copy(const void *_data, uint32_t _size)
	{
		const Memory *mem = alloc(_size);
		if (_size > 0)
		{
			std::memcpy(mem->data, _data, _size);
		}
		return mem;
	}

	const Memory *makeRef(const void *_data, uint32_t _size, ReleaseFn _releaseFn, void *_userData)
	{
		MemoryBlock *block = (MemoryBlock *)std::malloc(sizeof(MemoryBlock));
		block->mem.data = (uint8_t *)_data;
		block->mem.size = _size;
		block->release = _releaseFn;
		block->userData = _userData;
		return &block->mem;
	}

	// --- shaders, programs, uniforms ---------------------------------------------------------

	ShaderHandle createShader(const Memory *_mem)
	{
		ShaderHandle h = { zhgfx_create_shader(_mem->data, _mem->size) };
		Release(_mem);
		return h;
	}

	ProgramHandle createProgram(ShaderHandle _vsh, ShaderHandle _fsh, bool _destroyShaders)
	{
		ProgramHandle h = { zhgfx_create_program(_vsh.idx, _fsh.idx, _destroyShaders) };
		return h;
	}

	UniformHandle createUniform(const char *_name, UniformType::Enum _type, uint16_t _num)
	{
		UniformHandle h = { zhgfx_create_uniform(_name, uint8_t(_type), _num) };
		if (isValid(h))
		{
			g_uniforms[h.idx].floatsPerElement = _type == UniformType::Mat4 ? 16 : _type == UniformType::Mat3 ? 12 : 4;
			g_uniforms[h.idx].num = _num == 0 ? 1 : _num;
		}
		return h;
	}

	void setUniform(UniformHandle _handle, const void *_value, uint16_t _num)
	{
		if (!isValid(_handle) || _value == nullptr)
		{
			return;
		}
		const UniformInfo &u = g_uniforms[_handle.idx];
		const uint16_t num = _num == UINT16_MAX ? u.num : _num;
		zhgfx_set_uniform(_handle.idx, (const float *)_value, uint32_t(num) * u.floatsPerElement);
	}

	void destroy(ShaderHandle _handle) { zhgfx_destroy(ZHGFX_SHADER, _handle.idx); }
	void destroy(ProgramHandle _handle) { zhgfx_destroy(ZHGFX_PROGRAM, _handle.idx); }
	void destroy(UniformHandle _handle) { zhgfx_destroy(ZHGFX_UNIFORM, _handle.idx); }
	void destroy(TextureHandle _handle) { zhgfx_destroy(ZHGFX_TEXTURE, _handle.idx); }
	void destroy(FrameBufferHandle _handle) { zhgfx_destroy(ZHGFX_FRAME_BUFFER, _handle.idx); }
	void destroy(VertexBufferHandle _handle) { zhgfx_destroy(ZHGFX_VERTEX_BUFFER, _handle.idx); }
	void destroy(IndexBufferHandle _handle) { zhgfx_destroy(ZHGFX_INDEX_BUFFER, _handle.idx); }
	void destroy(DynamicVertexBufferHandle _handle) { zhgfx_destroy(ZHGFX_DYNAMIC_VERTEX_BUFFER, _handle.idx); }
	void destroy(DynamicIndexBufferHandle _handle) { zhgfx_destroy(ZHGFX_DYNAMIC_INDEX_BUFFER, _handle.idx); }

	void setName(ShaderHandle, const char *, int32_t) {}
	void setName(TextureHandle, const char *, int32_t) {}
	void setName(FrameBufferHandle, const char *, int32_t) {}
	void setName(VertexBufferHandle, const char *, int32_t) {}
	void setName(IndexBufferHandle, const char *, int32_t) {}

	// --- buffers ---------------------------------------------------------------------------------

	VertexBufferHandle createVertexBuffer(const Memory *_mem, const VertexLayout &_layout, uint16_t)
	{
		const ZhgfxLayout layout = ToLayout(_layout);
		VertexBufferHandle h = { zhgfx_create_vertex_buffer(_mem->data, _mem->size, &layout) };
		Release(_mem);
		return h;
	}

	IndexBufferHandle createIndexBuffer(const Memory *_mem, uint16_t _flags)
	{
		IndexBufferHandle h = { zhgfx_create_index_buffer(_mem->data, _mem->size, _flags) };
		Release(_mem);
		return h;
	}

	DynamicVertexBufferHandle createDynamicVertexBuffer(uint32_t _num, const VertexLayout &_layout, uint16_t)
	{
		const ZhgfxLayout layout = ToLayout(_layout);
		DynamicVertexBufferHandle h = { zhgfx_create_dynamic_vertex_buffer(_num, nullptr, 0, &layout) };
		return h;
	}

	DynamicVertexBufferHandle createDynamicVertexBuffer(const Memory *_mem, const VertexLayout &_layout, uint16_t)
	{
		const ZhgfxLayout layout = ToLayout(_layout);
		DynamicVertexBufferHandle h = { zhgfx_create_dynamic_vertex_buffer(0, _mem->data, _mem->size, &layout) };
		Release(_mem);
		return h;
	}

	DynamicIndexBufferHandle createDynamicIndexBuffer(uint32_t _num, uint16_t _flags)
	{
		DynamicIndexBufferHandle h = { zhgfx_create_dynamic_index_buffer(_num, nullptr, 0, _flags) };
		return h;
	}

	DynamicIndexBufferHandle createDynamicIndexBuffer(const Memory *_mem, uint16_t _flags)
	{
		DynamicIndexBufferHandle h = { zhgfx_create_dynamic_index_buffer(0, _mem->data, _mem->size, _flags) };
		Release(_mem);
		return h;
	}

	void update(DynamicVertexBufferHandle _handle, uint32_t _startVertex, const Memory *_mem)
	{
		zhgfx_update_dynamic_buffer(false, _handle.idx, _startVertex, _mem->data, _mem->size);
		Release(_mem);
	}

	void update(DynamicIndexBufferHandle _handle, uint32_t _startIndex, const Memory *_mem)
	{
		zhgfx_update_dynamic_buffer(true, _handle.idx, _startIndex, _mem->data, _mem->size);
		Release(_mem);
	}

	uint32_t getAvailTransientVertexBuffer(uint32_t _num, const VertexLayout &_layout)
	{
		return zhgfx_transient_avail(false, _num, _layout.m_stride);
	}

	uint32_t getAvailTransientIndexBuffer(uint32_t _num, bool _index32)
	{
		return zhgfx_transient_avail(true, _num, _index32 ? 4 : 2);
	}

	uint32_t getAvailInstanceDataBuffer(uint32_t _num, uint16_t _stride)
	{
		return zhgfx_transient_avail(false, _num, _stride);
	}

	void allocTransientVertexBuffer(TransientVertexBuffer *_tvb, uint32_t _num, const VertexLayout &_layout)
	{
		const ZhgfxLayout layout = ToLayout(_layout);
		uint32_t start = 0;
		uint8_t *data = zhgfx_transient_alloc(false, _num, _layout.m_stride, &start);
		_tvb->data = data;
		_tvb->size = data != nullptr ? _num * _layout.m_stride : 0;
		_tvb->startVertex = start;
		_tvb->stride = _layout.m_stride;
		_tvb->handle.idx = 0;
		_tvb->layoutHandle.idx = zhgfx_register_layout(&layout);
	}

	void allocTransientIndexBuffer(TransientIndexBuffer *_tib, uint32_t _num, bool _index32)
	{
		uint32_t start = 0;
		const uint32_t isize = _index32 ? 4 : 2;
		uint8_t *data = zhgfx_transient_alloc(true, _num, isize, &start);
		_tib->data = data;
		_tib->size = data != nullptr ? _num * isize : 0;
		_tib->startIndex = start;
		_tib->handle.idx = 0;
		_tib->isIndex16 = !_index32;
	}

	bool allocTransientBuffers(TransientVertexBuffer *_tvb, const VertexLayout &_layout, uint32_t _numVertices, TransientIndexBuffer *_tib, uint32_t _numIndices, bool _index32)
	{
		if (getAvailTransientVertexBuffer(_numVertices, _layout) < _numVertices
			|| getAvailTransientIndexBuffer(_numIndices, _index32) < _numIndices)
		{
			return false;
		}
		allocTransientVertexBuffer(_tvb, _numVertices, _layout);
		allocTransientIndexBuffer(_tib, _numIndices, _index32);
		return _tvb->data != nullptr && _tib->data != nullptr;
	}

	void allocInstanceDataBuffer(InstanceDataBuffer *_idb, uint32_t _num, uint16_t _stride)
	{
		uint32_t start = 0;
		uint8_t *data = zhgfx_transient_alloc(false, _num, _stride, &start);
		_idb->data = data;
		_idb->size = data != nullptr ? _num * _stride : 0;
		_idb->offset = start * _stride;
		_idb->num = data != nullptr ? _num : 0;
		_idb->stride = _stride;
		_idb->handle.idx = 0;
	}

	// --- textures and frame buffers ----------------------------------------------------------

	TextureHandle createTexture2D(uint16_t _width, uint16_t _height, bool _hasMips, uint16_t _numLayers, TextureFormat::Enum _format, uint64_t _flags, const Memory *_mem, uint64_t)
	{
		TextureHandle h = { zhgfx_create_texture_2d(_width, _height, _hasMips, _numLayers, uint32_t(_format), _flags,
			_mem != nullptr ? _mem->data : nullptr, _mem != nullptr ? _mem->size : 0) };
		Release(_mem);
		return h;
	}

	TextureHandle createTexture2D(BackbufferRatio::Enum _ratio, bool _hasMips, uint16_t _numLayers, TextureFormat::Enum _format, uint64_t _flags)
	{
		return createTexture2D(RatioSize(g_width, _ratio), RatioSize(g_height, _ratio), _hasMips, _numLayers, _format, _flags, nullptr, 0);
	}

	void updateTexture2D(TextureHandle _handle, uint16_t _layer, uint8_t _mip, uint16_t _x, uint16_t _y, uint16_t _width, uint16_t _height, const Memory *_mem, uint16_t _pitch)
	{
		zhgfx_update_texture_2d(_handle.idx, _layer, _mip, _x, _y, _width, _height, _mem->data, _mem->size, _pitch);
		Release(_mem);
	}

	FrameBufferHandle createFrameBuffer(uint8_t _num, const TextureHandle *_handles, bool _destroyTexture)
	{
		uint16_t handles[8];
		const uint8_t num = _num > 8 ? 8 : _num;
		for (uint8_t i = 0; i < num; ++i)
		{
			handles[i] = _handles[i].idx;
		}
		FrameBufferHandle h = { zhgfx_create_frame_buffer(num, handles, _destroyTexture) };
		return h;
	}

	FrameBufferHandle createFrameBuffer(uint8_t _num, const Attachment *_attachment, bool _destroyTexture)
	{
		uint16_t handles[8];
		const uint8_t num = _num > 8 ? 8 : _num;
		for (uint8_t i = 0; i < num; ++i)
		{
			handles[i] = _attachment[i].handle.idx;
		}
		FrameBufferHandle h = { zhgfx_create_frame_buffer(num, handles, _destroyTexture) };
		return h;
	}

	FrameBufferHandle createFrameBuffer(uint16_t _width, uint16_t _height, TextureFormat::Enum _format, uint64_t _textureFlags)
	{
		FrameBufferHandle h = { zhgfx_create_frame_buffer_sized(_width, _height, uint32_t(_format), _textureFlags) };
		return h;
	}

	FrameBufferHandle createFrameBuffer(BackbufferRatio::Enum _ratio, TextureFormat::Enum _format, uint64_t _textureFlags)
	{
		return createFrameBuffer(RatioSize(g_width, _ratio), RatioSize(g_height, _ratio), _format, _textureFlags);
	}

	// --- views -----------------------------------------------------------------------------------

	void setViewRect(ViewId _id, uint16_t _x, uint16_t _y, uint16_t _width, uint16_t _height)
	{
		zhgfx_set_view_rect(_id, _x, _y, _width, _height);
	}

	void setViewRect(ViewId _id, uint16_t _x, uint16_t _y, BackbufferRatio::Enum _ratio)
	{
		zhgfx_set_view_rect(_id, _x, _y, RatioSize(g_width, _ratio), RatioSize(g_height, _ratio));
	}

	void setViewClear(ViewId _id, uint16_t _flags, uint32_t _rgba, float _depth, uint8_t _stencil)
	{
		zhgfx_set_view_clear(_id, _flags, _rgba, _depth, _stencil);
	}

	void setViewMode(ViewId _id, ViewMode::Enum _mode)
	{
		zhgfx_set_view_mode(_id, uint8_t(_mode));
	}

	void setViewFrameBuffer(ViewId _id, FrameBufferHandle _handle)
	{
		zhgfx_set_view_frame_buffer(_id, _handle.idx);
	}

	void setViewTransform(ViewId _id, const void *_view, const void *_proj)
	{
		zhgfx_set_view_transform(_id, (const float *)_view, (const float *)_proj);
	}

	void setViewOrder(ViewId _id, uint16_t _num, const ViewId *_order)
	{
		zhgfx_set_view_order(_id, _num, _order);
	}

	void touch(ViewId _id)
	{
		zhgfx_touch(_id);
	}

	void requestScreenShot(FrameBufferHandle _handle, const char *_filePath)
	{
		zhgfx_request_screenshot(_handle.idx, _filePath);
	}

	// --- draw state ------------------------------------------------------------------------------

	void setState(uint64_t _state, uint32_t _rgba)
	{
		zhgfx_set_state(_state, _rgba);
	}

	void setStencil(uint32_t _fstencil, uint32_t _bstencil)
	{
		zhgfx_set_stencil(_fstencil, _bstencil);
	}

	uint32_t setTransform(const void *_mtx, uint16_t _num)
	{
		const uint32_t cache = zhgfx_alloc_transform((const float *)_mtx, _num);
		zhgfx_set_transform_cached(cache, _num);
		return cache;
	}

	void setTransform(uint32_t _cache, uint16_t _num)
	{
		zhgfx_set_transform_cached(_cache, _num);
	}

	void setIndexBuffer(IndexBufferHandle _handle)
	{
		zhgfx_set_index_buffer(ZHGFX_BUF_STATIC, _handle.idx, 0, UINT32_MAX);
	}

	void setIndexBuffer(IndexBufferHandle _handle, uint32_t _firstIndex, uint32_t _numIndices)
	{
		zhgfx_set_index_buffer(ZHGFX_BUF_STATIC, _handle.idx, _firstIndex, _numIndices);
	}

	void setIndexBuffer(DynamicIndexBufferHandle _handle)
	{
		zhgfx_set_index_buffer(ZHGFX_BUF_DYNAMIC, _handle.idx, 0, UINT32_MAX);
	}

	void setIndexBuffer(DynamicIndexBufferHandle _handle, uint32_t _firstIndex, uint32_t _numIndices)
	{
		zhgfx_set_index_buffer(ZHGFX_BUF_DYNAMIC, _handle.idx, _firstIndex, _numIndices);
	}

	void setIndexBuffer(const TransientIndexBuffer *_tib, uint32_t _firstIndex, uint32_t _numIndices)
	{
		const uint32_t isize = _tib->isIndex16 ? 2 : 4;
		const uint32_t avail = _tib->size / isize;
		const uint32_t first = _firstIndex < avail ? _firstIndex : avail;
		const uint32_t num = _numIndices < avail - first ? _numIndices : avail - first;
		zhgfx_set_index_buffer(_tib->isIndex16 ? ZHGFX_BUF_TRANSIENT : ZHGFX_BUF_TRANSIENT32, 0, _tib->startIndex + first, num);
	}

	void setIndexBuffer(const TransientIndexBuffer *_tib)
	{
		setIndexBuffer(_tib, 0, UINT32_MAX);
	}

	void setVertexBuffer(uint8_t _stream, VertexBufferHandle _handle)
	{
		zhgfx_set_vertex_buffer(_stream, ZHGFX_BUF_STATIC, _handle.idx, 0, UINT32_MAX, kInvalidHandle);
	}

	void setVertexBuffer(uint8_t _stream, VertexBufferHandle _handle, uint32_t _startVertex, uint32_t _numVertices, VertexLayoutHandle _layoutHandle)
	{
		zhgfx_set_vertex_buffer(_stream, ZHGFX_BUF_STATIC, _handle.idx, _startVertex, _numVertices, _layoutHandle.idx);
	}

	void setVertexBuffer(uint8_t _stream, DynamicVertexBufferHandle _handle)
	{
		zhgfx_set_vertex_buffer(_stream, ZHGFX_BUF_DYNAMIC, _handle.idx, 0, UINT32_MAX, kInvalidHandle);
	}

	void setVertexBuffer(uint8_t _stream, DynamicVertexBufferHandle _handle, uint32_t _startVertex, uint32_t _numVertices, VertexLayoutHandle _layoutHandle)
	{
		zhgfx_set_vertex_buffer(_stream, ZHGFX_BUF_DYNAMIC, _handle.idx, _startVertex, _numVertices, _layoutHandle.idx);
	}

	void setVertexBuffer(uint8_t _stream, const TransientVertexBuffer *_tvb, uint32_t _startVertex, uint32_t _numVertices, VertexLayoutHandle _layoutHandle)
	{
		const uint32_t avail = _tvb->stride != 0 ? _tvb->size / _tvb->stride : 0;
		const uint32_t start = _startVertex < avail ? _startVertex : avail;
		const uint32_t num = _numVertices < avail - start ? _numVertices : avail - start;
		const uint16_t layout = isValid(_layoutHandle) ? _layoutHandle.idx : _tvb->layoutHandle.idx;
		zhgfx_set_vertex_buffer(_stream, ZHGFX_BUF_TRANSIENT, layout, _tvb->startVertex + start, num, kInvalidHandle);
	}

	void setVertexBuffer(uint8_t _stream, const TransientVertexBuffer *_tvb)
	{
		setVertexBuffer(_stream, _tvb, 0, UINT32_MAX, BGFX_INVALID_HANDLE);
	}

	void setInstanceDataBuffer(const InstanceDataBuffer *_idb, uint32_t _start, uint32_t _num)
	{
		if (_idb->stride == 0)
		{
			return;
		}
		const uint32_t start = _start < _idb->num ? _start : _idb->num;
		const uint32_t num = _num < _idb->num - start ? _num : _idb->num - start;
		zhgfx_set_instance_data(ZHGFX_BUF_TRANSIENT, 0, _idb->offset / _idb->stride + start, num, _idb->stride);
	}

	void setInstanceDataBuffer(const InstanceDataBuffer *_idb)
	{
		setInstanceDataBuffer(_idb, 0, UINT32_MAX);
	}

	void setInstanceDataBuffer(VertexBufferHandle _handle, uint32_t _startVertex, uint32_t _num)
	{
		zhgfx_set_instance_data(ZHGFX_BUF_STATIC, _handle.idx, _startVertex, _num, 0);
	}

	void setInstanceDataBuffer(DynamicVertexBufferHandle _handle, uint32_t _startVertex, uint32_t _num)
	{
		zhgfx_set_instance_data(ZHGFX_BUF_DYNAMIC, _handle.idx, _startVertex, _num, 0);
	}

	void setTexture(uint8_t _stage, UniformHandle, TextureHandle _handle, uint32_t _flags)
	{
		zhgfx_set_texture(_stage, _handle.idx, _flags);
	}

	void submit(ViewId _id, ProgramHandle _program, uint32_t _depth, uint8_t _flags)
	{
		zhgfx_submit(_id, _program.idx, _depth, _flags);
	}

	void discard(uint8_t _flags)
	{
		zhgfx_discard(_flags);
	}

	void blit(ViewId _id, TextureHandle _dst, uint8_t _dstMip, uint16_t _dstX, uint16_t _dstY, uint16_t _dstZ, TextureHandle _src, uint8_t _srcMip, uint16_t _srcX, uint16_t _srcY, uint16_t _srcZ, uint16_t _width, uint16_t _height, uint16_t _depth)
	{
		zhgfx_blit(_id, _dst.idx, _dstMip, _dstX, _dstY, _dstZ, _src.idx, _srcMip, _srcX, _srcY, _srcZ, _width, _height, _depth);
	}

	void blit(ViewId _id, TextureHandle _dst, uint16_t _dstX, uint16_t _dstY, TextureHandle _src, uint16_t _srcX, uint16_t _srcY, uint16_t _width, uint16_t _height)
	{
		blit(_id, _dst, 0, _dstX, _dstY, 0, _src, 0, _srcX, _srcY, 0, _width, _height, 1);
	}
}
