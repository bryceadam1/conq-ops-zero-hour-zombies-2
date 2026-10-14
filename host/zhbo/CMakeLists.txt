# Conquer Ops engine glue. Added to the GeneralsGameCode build by host/patches (root CMakeLists.txt)
# when ZHBO_GLUE_DIR is set; `cargo xtask build` sets it together with ZHBO_CORE_LIB.

if(NOT ZHBO_CORE_LIB)
    message(FATAL_ERROR "ZHBO_CORE_LIB must point at zhbo_core.lib (built by cargo for i686-pc-windows-msvc)")
endif()

target_sources(z_gameengine PRIVATE
    ${CMAKE_CURRENT_LIST_DIR}/ZhboBridge.cpp
    ${CMAKE_CURRENT_LIST_DIR}/ZhboBridge.h
    ${CMAKE_CURRENT_LIST_DIR}/zhbo_core.h
)

# Patched engine files in z_gameengine and z_gameenginedevice include ZhboBridge.h.
target_include_directories(z_gameengine PUBLIC ${CMAKE_CURRENT_LIST_DIR})
target_include_directories(z_gameenginedevice PUBLIC ${CMAKE_CURRENT_LIST_DIR})

# The Rust core and the system libraries Rust's std needs (from `--print native-static-libs`).
target_link_libraries(z_gameengine PUBLIC ${ZHBO_CORE_LIB} advapi32 ntdll userenv ws2_32 dbghelp)
