# Conquer Ops engine glue. Added to the GeneralsGameCode build by host/patches (root CMakeLists.txt)
# when ZHBO_GLUE_DIR is set; `cargo xtask build` sets it together with ZHBO_CORE_LIB.
#
# Builds:
#   Windows x86 DX8 (preset win32):   ZHBO_CORE_LIB = .../i686-pc-windows-msvc/release/zhbo_core.lib
#   Windows x64 wgpu (preset win64):  ZHBO_CORE_LIB = .../x86_64-pc-windows-msvc/release/zhbo_core.lib
#   macOS / Linux wgpu (native):      ZHBO_CORE_LIB = .../<host triple>/release/libzhbo_core.a
# The wgpu builds also need the core built with `--features renderer` (see below).

if(NOT ZHBO_CORE_LIB)
    message(FATAL_ERROR "ZHBO_CORE_LIB must point at the zhbo-core static library built by cargo")
endif()

target_sources(z_gameengine PRIVATE
    ${CMAKE_CURRENT_LIST_DIR}/ZhboBridge.cpp
    ${CMAKE_CURRENT_LIST_DIR}/ZhboBridge.h
    ${CMAKE_CURRENT_LIST_DIR}/ZhboPlatform.cpp
    ${CMAKE_CURRENT_LIST_DIR}/ZhboPlatform.h
    ${CMAKE_CURRENT_LIST_DIR}/zhbo_core.h
)

# The first-person viewmodel draws through WW3D, so it lives in the device library.
target_sources(z_gameenginedevice PRIVATE
    ${CMAKE_CURRENT_LIST_DIR}/ZhboViewmodel.cpp
)

# Patched engine files in z_gameengine and z_gameenginedevice include ZhboBridge.h.
target_include_directories(z_gameengine PUBLIC ${CMAKE_CURRENT_LIST_DIR})
target_include_directories(z_gameenginedevice PUBLIC ${CMAKE_CURRENT_LIST_DIR})

# ZhboPlatform uses SDL3 for window focus, keys and mouse capture where the engine runs on SDL3.
if(SAGE_USE_SDL3)
    target_link_libraries(z_gameengine PRIVATE sdl3lib)
endif()

# The Rust core and the system libraries it needs, from
# `cargo rustc -p zhbo-core --release [--features renderer] --target <triple> --crate-type staticlib
#  -- --print native-static-libs` (panic=abort).
if(WIN32)
    # opengl32: wgpu's GL backend (only linked by the renderer, harmless otherwise).
    target_link_libraries(z_gameengine PUBLIC ${ZHBO_CORE_LIB} advapi32 ntdll userenv ws2_32 dbghelp opengl32)
elseif(APPLE)
    target_link_libraries(z_gameengine PUBLIC ${ZHBO_CORE_LIB}
        "-framework QuartzCore" "-framework CoreGraphics" "-framework Metal"
        "-framework Foundation" "-framework CoreFoundation" objc iconv)
else()
    target_link_libraries(z_gameengine PUBLIC ${ZHBO_CORE_LIB} dl pthread m rt util)
endif()

# GGC_BGFX_RENDERER=wgpu: the bgfx API on the Rust renderer (zhbo-gfx, inside ZHBO_CORE_LIB, built
# with `--features renderer`). corei_ww3d2 links zhbo_bgfx_wgpu instead of the bgfx library (see
# host/patches/0002); bgfx's headers and bx/bimg still come from the engine's bgfx.cmake.
if(GGC_RENDER_BACKEND STREQUAL "bgfx" AND GGC_BGFX_RENDERER STREQUAL "wgpu")
    add_library(zhbo_bgfx_wgpu STATIC
        ${CMAKE_CURRENT_LIST_DIR}/bgfx_wgpu.cpp
        ${CMAKE_CURRENT_LIST_DIR}/zhbo_gfx.h
    )
    target_include_directories(zhbo_bgfx_wgpu PUBLIC $<TARGET_PROPERTY:bgfx,INTERFACE_INCLUDE_DIRECTORIES>)
    target_compile_definitions(zhbo_bgfx_wgpu PUBLIC $<TARGET_PROPERTY:bgfx,INTERFACE_COMPILE_DEFINITIONS>)
    target_link_libraries(zhbo_bgfx_wgpu PUBLIC ${ZHBO_CORE_LIB} bx)
    # core_config carries the engine-wide defines (SAGE_USE_SDL3, ...).
    target_link_libraries(zhbo_bgfx_wgpu PRIVATE core_config)
    if(SAGE_USE_SDL3)
        target_link_libraries(zhbo_bgfx_wgpu PRIVATE sdl3lib)
    endif()
    # Never build the real bgfx library in this configuration.
    set_target_properties(bgfx PROPERTIES EXCLUDE_FROM_ALL ON)
endif()
