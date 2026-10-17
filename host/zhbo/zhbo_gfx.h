// C ABI of the Rust renderer (crates/zhbo-gfx/src/ffi.rs). Used only by bgfx_wgpu.cpp.
//
// Resource handles are opaque uint16_t values created by zhgfx_create_* and passed back to
// zhgfx_set_*/zhgfx_destroy. Internally the Rust renderer packs a 4-bit generation into the high
// bits ((generation << 12) | slot) so a handle that outlives its resource is rejected instead of
// aliasing a recreated one; treat handles as opaque and never persist them across a renderer
// restart. 0xFFFF is invalid (bgfx's kInvalidHandle) and is never handed out.
#pragma once

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct ZhgfxInit
{
	uint32_t kind; // 0 headless, 1 Win32 HWND, 2 CAMetalLayer, 3 X11, 4 Wayland
	void *window;
	void *display;
	uint32_t width;
	uint32_t height;
	uint32_t reset;
	uint32_t transient_vb_size;
	uint32_t transient_ib_size;
} ZhgfxInit;

typedef struct ZhgfxLayout
{
	uint16_t stride;
	uint16_t offset[18];
	uint16_t attributes[18];
} ZhgfxLayout;

typedef struct ZhgfxStats
{
	uint32_t num_draw;
	uint32_t num_blit;
	uint32_t num_textures;
	uint32_t num_frame_buffers;
	uint32_t transient_vb_used;
	uint32_t transient_ib_used;
	uint32_t errors;
	uint64_t cpu_frame_ns;
} ZhgfxStats;

typedef void (*ZhgfxTraceCb)(const char *msg);
typedef void (*ZhgfxScreenshotCb)(const char *path, uint32_t width, uint32_t height, uint32_t pitch, const uint8_t *data, uint32_t size);

enum
{
	ZHGFX_VERTEX_BUFFER = 0,
	ZHGFX_INDEX_BUFFER = 1,
	ZHGFX_DYNAMIC_VERTEX_BUFFER = 2,
	ZHGFX_DYNAMIC_INDEX_BUFFER = 3,
	ZHGFX_SHADER = 4,
	ZHGFX_PROGRAM = 5,
	ZHGFX_TEXTURE = 6,
	ZHGFX_FRAME_BUFFER = 7,
	ZHGFX_UNIFORM = 8,
};

enum
{
	ZHGFX_BUF_STATIC = 0,
	ZHGFX_BUF_DYNAMIC = 1,
	ZHGFX_BUF_TRANSIENT = 2,
	ZHGFX_BUF_TRANSIENT32 = 3,
};

void zhgfx_set_callbacks(ZhgfxTraceCb trace, ZhgfxScreenshotCb screenshot);
bool zhgfx_init(const ZhgfxInit *init);
void zhgfx_shutdown(void);
void zhgfx_reset(uint32_t width, uint32_t height, uint32_t flags);
uint32_t zhgfx_frame(void);
void zhgfx_adapter_name(char *out, uint32_t cap);
uint32_t zhgfx_format_caps(uint32_t bgfx_format);
uint32_t zhgfx_max_texture_size(void);
void zhgfx_stats(ZhgfxStats *out);

uint16_t zhgfx_create_shader(const uint8_t *data, uint32_t size);
uint16_t zhgfx_create_program(uint16_t vs, uint16_t fs, bool destroy_shaders);
uint16_t zhgfx_create_uniform(const char *name, uint8_t type, uint16_t num);
void zhgfx_set_uniform(uint16_t h, const float *value, uint32_t floats);
void zhgfx_destroy(uint32_t kind, uint16_t h);

uint16_t zhgfx_register_layout(const ZhgfxLayout *layout);
uint16_t zhgfx_create_vertex_buffer(const uint8_t *data, uint32_t size, const ZhgfxLayout *layout);
uint16_t zhgfx_create_index_buffer(const uint8_t *data, uint32_t size, uint16_t flags);
uint16_t zhgfx_create_dynamic_vertex_buffer(uint32_t num, const uint8_t *data, uint32_t size, const ZhgfxLayout *layout);
uint16_t zhgfx_create_dynamic_index_buffer(uint32_t num, const uint8_t *data, uint32_t size, uint16_t flags);
void zhgfx_update_dynamic_buffer(bool index, uint16_t h, uint32_t start, const uint8_t *data, uint32_t size);
uint32_t zhgfx_transient_avail(bool index, uint32_t num, uint32_t stride);
uint8_t *zhgfx_transient_alloc(bool index, uint32_t num, uint32_t stride, uint32_t *start);

uint16_t zhgfx_create_texture_2d(uint32_t width, uint32_t height, bool has_mips, uint32_t layers, uint32_t format, uint64_t flags, const uint8_t *data, uint32_t size);
void zhgfx_update_texture_2d(uint16_t h, uint32_t layer, uint32_t mip, uint32_t x, uint32_t y, uint32_t w, uint32_t height, const uint8_t *data, uint32_t size, uint32_t pitch);
uint16_t zhgfx_create_frame_buffer(uint8_t num, const uint16_t *textures, bool destroy_textures);
uint16_t zhgfx_create_frame_buffer_sized(uint32_t width, uint32_t height, uint32_t format, uint64_t flags);

void zhgfx_set_view_rect(uint16_t v, uint16_t x, uint16_t y, uint16_t w, uint16_t h);
void zhgfx_set_view_clear(uint16_t v, uint16_t flags, uint32_t rgba, float depth, uint8_t stencil);
void zhgfx_set_view_mode(uint16_t v, uint8_t mode);
void zhgfx_set_view_frame_buffer(uint16_t v, uint16_t fb);
void zhgfx_set_view_transform(uint16_t v, const float *view, const float *proj);
void zhgfx_set_view_order(uint16_t first, uint16_t num, const uint16_t *order);
void zhgfx_touch(uint16_t v);
void zhgfx_request_screenshot(uint16_t fb, const char *path);

void zhgfx_set_state(uint64_t state, uint32_t rgba);
void zhgfx_set_stencil(uint32_t front, uint32_t back);
uint32_t zhgfx_alloc_transform(const float *mtx, uint16_t num);
void zhgfx_set_transform_cached(uint32_t cache, uint16_t num);
void zhgfx_set_index_buffer(uint32_t kind, uint16_t h, uint32_t first, uint32_t num);
void zhgfx_set_vertex_buffer(uint8_t stream, uint32_t kind, uint16_t h, uint32_t start, uint32_t num, uint16_t layout);
void zhgfx_set_instance_data(uint32_t kind, uint16_t h, uint32_t start, uint32_t num, uint32_t stride);
void zhgfx_set_texture(uint8_t stage, uint16_t h, uint32_t flags);
void zhgfx_submit(uint16_t v, uint16_t program, uint32_t depth, uint8_t discard);
void zhgfx_discard(uint8_t flags);
void zhgfx_blit(uint16_t v, uint16_t dst, uint8_t dst_mip, uint16_t dx, uint16_t dy, uint16_t dz, uint16_t src, uint8_t src_mip, uint16_t sx, uint16_t sy, uint16_t sz, uint16_t w, uint16_t h, uint16_t d);

#ifdef __cplusplus
}
#endif
