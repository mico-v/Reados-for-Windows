cmake_minimum_required(VERSION 3.20)

project(msp_ffi_cpp LANGUAGES CXX)

include(CTest)

set(MSP_FFI_HEADER
    "${CMAKE_CURRENT_LIST_DIR}/../../include/msp_ffi.h"
    CACHE FILEPATH
    "Path to the public msp_ffi.h header")
set(MSP_FFI_DLL "" CACHE FILEPATH "Path to the real msp_ffi DLL")
set(MSP_FFI_IMPORT_LIBRARY "" CACHE FILEPATH "Path to the real msp_ffi import library")

if(NOT EXISTS "${MSP_FFI_HEADER}")
    message(FATAL_ERROR "MSP_FFI_HEADER does not exist: ${MSP_FFI_HEADER}")
endif()

# Prefer already-built debug or release artifacts when this directory is
# configured directly from the repository. Explicit cache values always win.
if(NOT MSP_FFI_DLL)
    foreach(candidate
        "${CMAKE_CURRENT_LIST_DIR}/../../target/debug/msp_ffi.dll"
        "${CMAKE_CURRENT_LIST_DIR}/../../target/release/msp_ffi.dll")
        if(EXISTS "${candidate}")
            set(MSP_FFI_DLL "${candidate}" CACHE FILEPATH "Path to the real msp_ffi DLL" FORCE)
            break()
        endif()
    endforeach()
endif()

if(NOT MSP_FFI_IMPORT_LIBRARY)
    foreach(candidate
        "${CMAKE_CURRENT_LIST_DIR}/../../target/debug/msp_ffi.dll.lib"
        "${CMAKE_CURRENT_LIST_DIR}/../../target/release/msp_ffi.dll.lib")
        if(EXISTS "${candidate}")
            set(MSP_FFI_IMPORT_LIBRARY
                "${candidate}"
                CACHE FILEPATH
                "Path to the real msp_ffi import library"
                FORCE)
            break()
        endif()
    endforeach()
endif()

get_filename_component(MSP_FFI_INCLUDE_DIR "${MSP_FFI_HEADER}" DIRECTORY)

add_library(msp_ffi_cpp INTERFACE)
add_library(msp_ffi::cpp ALIAS msp_ffi_cpp)
add_library(msp_ffi::Cpp ALIAS msp_ffi_cpp)
target_compile_features(msp_ffi_cpp INTERFACE cxx_std_20)
target_include_directories(msp_ffi_cpp INTERFACE
    "${CMAKE_CURRENT_LIST_DIR}/include"
    "${MSP_FFI_INCLUDE_DIR}")

set(MSP_FFI_RUNTIME_AVAILABLE FALSE)
if(MSP_FFI_DLL AND MSP_FFI_IMPORT_LIBRARY
   AND EXISTS "${MSP_FFI_DLL}"
   AND EXISTS "${MSP_FFI_IMPORT_LIBRARY}")
    add_library(msp_ffi_runtime SHARED IMPORTED GLOBAL)
    add_library(msp_ffi::runtime ALIAS msp_ffi_runtime)
    set_target_properties(msp_ffi_runtime PROPERTIES
        IMPORTED_LOCATION "${MSP_FFI_DLL}"
        IMPORTED_IMPLIB "${MSP_FFI_IMPORT_LIBRARY}"
        INTERFACE_INCLUDE_DIRECTORIES "${MSP_FFI_INCLUDE_DIR}")
    target_link_libraries(msp_ffi_cpp INTERFACE msp_ffi_runtime)
    set(MSP_FFI_RUNTIME_AVAILABLE TRUE)
else()
    message(STATUS
        "msp_ffi runtime DLL/import library unavailable; C++ smoke will compile only")
endif()

if(BUILD_TESTING)
    if(MSP_FFI_RUNTIME_AVAILABLE)
        add_executable(msp_ffi_cpp_consumer_smoke tests/cpp_consumer_smoke.cpp)
        target_link_libraries(msp_ffi_cpp_consumer_smoke PRIVATE msp_ffi_cpp)
        target_compile_features(msp_ffi_cpp_consumer_smoke PRIVATE cxx_std_20)
        add_custom_command(TARGET msp_ffi_cpp_consumer_smoke POST_BUILD
            COMMAND "${CMAKE_COMMAND}" -E copy_if_different
                "${MSP_FFI_DLL}"
                "$<TARGET_FILE_DIR:msp_ffi_cpp_consumer_smoke>")
        add_test(NAME msp_ffi_cpp_consumer_smoke
            COMMAND msp_ffi_cpp_consumer_smoke)
    else()
        add_library(msp_ffi_cpp_consumer_compile OBJECT tests/cpp_consumer_smoke.cpp)
        target_link_libraries(msp_ffi_cpp_consumer_compile PRIVATE msp_ffi_cpp)
        target_compile_features(msp_ffi_cpp_consumer_compile PRIVATE cxx_std_20)
    endif()
endif()
