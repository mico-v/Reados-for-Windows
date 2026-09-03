cmake_minimum_required(VERSION 3.22.1)
project(msp_command_runtime_ffi_jni LANGUAGES CXX)

if(NOT DEFINED RUST_FFI_LIBRARY OR NOT EXISTS "${RUST_FFI_LIBRARY}")
    message(FATAL_ERROR "RUST_FFI_LIBRARY must point to the real Android Rust shared library")
endif()
if(NOT DEFINED RUST_FFI_INCLUDE_DIR OR NOT EXISTS "${RUST_FFI_INCLUDE_DIR}/msp_command_runtime_ffi.h")
    message(FATAL_ERROR "RUST_FFI_INCLUDE_DIR must contain msp_command_runtime_ffi.h")
endif()

add_library(msp_command_runtime_ffi SHARED IMPORTED GLOBAL)
set_target_properties(msp_command_runtime_ffi PROPERTIES
    IMPORTED_LOCATION "${RUST_FFI_LIBRARY}"
    IMPORTED_NO_SONAME FALSE
    IMPORTED_SONAME "libmsp_command_runtime_ffi.so"
    INTERFACE_INCLUDE_DIRECTORIES "${RUST_FFI_INCLUDE_DIR}"
)

add_library(msp_command_runtime_ffi_jni SHARED msp_command_runtime_ffi_jni.cpp)
target_compile_features(msp_command_runtime_ffi_jni PRIVATE cxx_std_17)
target_compile_options(msp_command_runtime_ffi_jni PRIVATE
    -fvisibility=hidden
    -Wall
    -Wextra
    -Werror
)
target_include_directories(msp_command_runtime_ffi_jni PRIVATE "${RUST_FFI_INCLUDE_DIR}")
target_link_libraries(msp_command_runtime_ffi_jni PRIVATE msp_command_runtime_ffi log)

# Keep the shim's dependency on the Rust artifact explicit. The AAR packages
# both libmsp_command_runtime_ffi_jni.so and libmsp_command_runtime_ffi.so.
set_target_properties(msp_command_runtime_ffi_jni PROPERTIES
    OUTPUT_NAME "msp_command_runtime_ffi_jni"
    CXX_VISIBILITY_PRESET hidden
    VISIBILITY_INLINES_HIDDEN YES
)
