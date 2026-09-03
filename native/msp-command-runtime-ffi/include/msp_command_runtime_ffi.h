#ifndef READOS_MSP_COMMAND_RUNTIME_FFI_H_INCLUDED
#define READOS_MSP_COMMAND_RUNTIME_FFI_H_INCLUDED

#include <stddef.h>
#include <stdint.h>

#define MCR_FFI_ABI_VERSION UINT32_C(1)
#define MCR_FFI_REQUEST_SCHEMA_VERSION UINT32_C(1)
#define MCR_FFI_HEADER_VERSION_STRING "0.1.0"

#define MCR_FFI_MAX_JSON_REQUEST_BYTES \
    (UINT64_C(4) * UINT64_C(1024) * UINT64_C(1024))
#define MCR_FFI_MAX_COMMAND_BYTES \
    (UINT64_C(128) * UINT64_C(1024))
#define MCR_FFI_MAX_CWD_BYTES \
    (UINT64_C(4) * UINT64_C(1024))
#define MCR_FFI_MAX_FILE_BYTES \
    (UINT64_C(8) * UINT64_C(1024) * UINT64_C(1024))
#define MCR_FFI_MAX_WORKSPACE_BYTES \
    (UINT64_C(64) * UINT64_C(1024) * UINT64_C(1024))
#define MCR_FFI_MAX_WORKSPACE_FILES UINT64_C(65536)
#define MCR_FFI_MAX_LIST_ENTRIES UINT64_C(65536)
#define MCR_FFI_MAX_STDIN_BYTES \
    (UINT64_C(2) * UINT64_C(1024) * UINT64_C(1024))
#define MCR_FFI_MAX_OUTPUT_BYTES \
    (UINT64_C(2) * UINT64_C(1024) * UINT64_C(1024))
#define MCR_FFI_MAX_DIAGNOSTIC_BYTES UINT64_C(4096)
#define MCR_FFI_MAX_VARIABLES UINT64_C(256)
#define MCR_FFI_MAX_VARIABLE_NAME_BYTES UINT64_C(64)
#define MCR_FFI_MAX_VARIABLE_VALUE_BYTES \
    (UINT64_C(64) * UINT64_C(1024))
#define MCR_FFI_MAX_VARIABLE_TOTAL_BYTES \
    (UINT64_C(256) * UINT64_C(1024))

#define MCR_FFI_STATUS_OK INT32_C(0)
#define MCR_FFI_STATUS_INVALID_ARGUMENT INT32_C(1)
#define MCR_FFI_STATUS_LIMIT_EXCEEDED INT32_C(2)
#define MCR_FFI_STATUS_INTERNAL_ERROR INT32_C(3)
#define MCR_FFI_STATUS_PANIC INT32_C(4)

#if defined(_WIN32)
    #if defined(MCR_FFI_BUILD)
        #define MCR_FFI_API __declspec(dllexport)
    #else
        #define MCR_FFI_API __declspec(dllimport)
    #endif
    #define MCR_FFI_CALL __cdecl
#elif defined(__GNUC__) || defined(__clang__)
    #define MCR_FFI_API __attribute__((visibility("default")))
    #define MCR_FFI_CALL
#else
    #define MCR_FFI_API
    #define MCR_FFI_CALL
#endif

#ifdef __cplusplus
extern "C" {
#endif

typedef struct MspCommandRuntimeFfiRuntime
    MspCommandRuntimeFfiRuntime;

typedef struct MspCommandRuntimeFfiWorkspace
    MspCommandRuntimeFfiWorkspace;

typedef struct MspCommandRuntimeFfiResult
    MspCommandRuntimeFfiResult;

/*
 * ABI metadata is static storage. The returned version pointer is not freed.
 * version_data may be called with a null length_out.
 */
MCR_FFI_API uint32_t MCR_FFI_CALL
msp_command_runtime_ffi_abi_version(void);

MCR_FFI_API const uint8_t *MCR_FFI_CALL
msp_command_runtime_ffi_version_data(size_t *length_out);

/*
 * The returned handles are owned by the caller. Null is a creation failure.
 * A handle must be freed exactly once with the matching free function.
 */
MCR_FFI_API MspCommandRuntimeFfiRuntime *MCR_FFI_CALL
msp_command_runtime_ffi_runtime_create(void);

MCR_FFI_API void MCR_FFI_CALL
msp_command_runtime_ffi_runtime_free(
    MspCommandRuntimeFfiRuntime *runtime);

MCR_FFI_API MspCommandRuntimeFfiWorkspace *MCR_FFI_CALL
msp_command_runtime_ffi_workspace_create(void);

MCR_FFI_API void MCR_FFI_CALL
msp_command_runtime_ffi_workspace_free(
    MspCommandRuntimeFfiWorkspace *workspace);

/*
 * path_ptr/path_len is a canonical non-root virtual UTF-8 path.
 * It must not contain NUL, backslash, drive/UNC syntax, traversal,
 * duplicate separators, control characters, or a .msp component.
 *
 * data_ptr/data_len is arbitrary binary data. A null data_ptr is valid
 * only when data_len is zero. The bytes are copied before return.
 */
MCR_FFI_API int32_t MCR_FFI_CALL
msp_command_runtime_ffi_workspace_put_file(
    MspCommandRuntimeFfiWorkspace *workspace,
    const uint8_t *path_ptr,
    size_t path_len,
    const uint8_t *data_ptr,
    size_t data_len);

/*
 * request_json is an explicit-length UTF-8 JSON document. It is not
 * NUL-terminated. Raw NUL bytes in request_json are rejected. NUL decoded
 * in command, cwd, variable names, or variable values is also rejected.
 * The stdinBase64 field is decoded as arbitrary binary: decoded stdin may
 * contain NUL and 0xFF, and those bytes are preserved. The workspace file
 * data above has the same binary-safe behavior. The workspace and runtime
 * are borrowed for the duration of the call.
 *
 * The returned result owns independent copies of all output and diagnostic
 * bytes, so runtime/workspace handles may be freed after this call.
 */
MCR_FFI_API MspCommandRuntimeFfiResult *MCR_FFI_CALL
msp_command_runtime_ffi_execute_json(
    const MspCommandRuntimeFfiRuntime *runtime,
    const MspCommandRuntimeFfiWorkspace *workspace,
    const uint8_t *request_json,
    size_t request_len);

/*
 * Result accessors borrow storage owned by result until result_free.
 * stdout and stderr may contain arbitrary bytes and are not NUL-terminated.
 * diagnostic_data is compact UTF-8 JSON and is also not NUL-terminated.
 * Each length_out may be null. A null result returns exit code 2 and an
 * empty, non-owned data sentinel with length zero.
 */
MCR_FFI_API int32_t MCR_FFI_CALL
msp_command_runtime_ffi_result_exit_code(
    const MspCommandRuntimeFfiResult *result);

MCR_FFI_API const uint8_t *MCR_FFI_CALL
msp_command_runtime_ffi_result_stdout_data(
    const MspCommandRuntimeFfiResult *result,
    size_t *length_out);

MCR_FFI_API const uint8_t *MCR_FFI_CALL
msp_command_runtime_ffi_result_stderr_data(
    const MspCommandRuntimeFfiResult *result,
    size_t *length_out);

MCR_FFI_API const uint8_t *MCR_FFI_CALL
msp_command_runtime_ffi_result_diagnostic_data(
    const MspCommandRuntimeFfiResult *result,
    size_t *length_out);

MCR_FFI_API void MCR_FFI_CALL
msp_command_runtime_ffi_result_free(
    MspCommandRuntimeFfiResult *result);

#ifdef __cplusplus
}
#endif

#endif
