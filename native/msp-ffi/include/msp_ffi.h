#ifndef MSP_FFI_H_INCLUDED
#define MSP_FFI_H_INCLUDED

#include <stddef.h>
#include <stdint.h>

#define MSP_FFI_ABI_VERSION UINT32_C(1)
#define MSP_FFI_HEADER_VERSION_STRING "0.1.0"
#define MSP_FFI_MAX_COMMAND_BYTES (UINT64_C(64) * UINT64_C(1024))
#define MSP_FFI_MAX_PATH_BYTES (UINT64_C(4) * UINT64_C(1024))
#define MSP_FFI_MAX_FILE_BYTES (UINT64_C(8) * UINT64_C(1024) * UINT64_C(1024))
#define MSP_FFI_MAX_WORKSPACE_BYTES (UINT64_C(64) * UINT64_C(1024) * UINT64_C(1024))
#define MSP_FFI_MAX_WORKSPACE_NODES UINT64_C(65536)
#define MSP_FFI_MAX_READ_BYTES (UINT64_C(1) * UINT64_C(1024) * UINT64_C(1024))
#define MSP_FFI_MAX_RESULT_BYTES (UINT64_C(8) * UINT64_C(1024) * UINT64_C(1024))
#define MSP_FFI_MAX_LIST_ENTRIES UINT64_C(65536)

#define MSP_FFI_STATUS_OK INT32_C(0)
#define MSP_FFI_STATUS_ERROR INT32_C(1)
#define MSP_FFI_STATUS_INVALID_ARGUMENT INT32_C(2)
#define MSP_FFI_STATUS_LIMIT_EXCEEDED INT32_C(3)
#define MSP_FFI_STATUS_PANIC INT32_C(4)

#if defined(_WIN32)
#if defined(MSP_FFI_BUILD)
#define MSP_FFI_API __declspec(dllexport)
#else
#define MSP_FFI_API __declspec(dllimport)
#endif
#define MSP_FFI_CALL __cdecl
#elif defined(__GNUC__) || defined(__clang__)
#define MSP_FFI_API __attribute__((visibility("default")))
#define MSP_FFI_CALL
#else
#define MSP_FFI_API
#define MSP_FFI_CALL
#endif

#ifdef __cplusplus
extern "C" {
#endif

/*
 * Public ABI rules
 *
 * - MspSession, MspWorkspace, and MspResult are opaque Rust-owned handles.
 * - Every const char* input is a bounded, NUL-terminated UTF-8 string. A
 *   missing terminator, invalid UTF-8 sequence, embedded NUL before the end of
 *   the logical value, or length over the documented cap is rejected.
 * - Workspace paths are virtual POSIX-style paths only. They are normalized
 *   and checked by the MSP core; drive, UNC, device, backslash, ADS, hidden,
 *   and traversal forms are never treated as host paths.
 * - Workspace data is an in-memory virtual workspace. No function accepts a
 *   host root, opens a host path, reads a host environment, or launches a
 *   caller-selected process.
 * - Result buffers are borrowed until msp_result_free. They may contain
 *   arbitrary bytes and are not NUL-terminated. The caller must not free or
 *   write them.
 * - All exported calls contain Rust panics. A result call returns an owned
 *   failure result; status, pointer, and destroy calls return a documented
 *   sentinel. A sentinel does not make forged or dangling handles valid.
 */

typedef struct MspSession MspSession;
typedef struct MspWorkspace MspWorkspace;
typedef struct MspResult MspResult;

/* Runtime information lives in static storage and must not be freed. */
MSP_FFI_API uint32_t MSP_FFI_CALL msp_runtime_abi_version(void);
MSP_FFI_API const char *MSP_FFI_CALL msp_runtime_version(void);

/* Compatibility spellings used by early SDK consumers. */
MSP_FFI_API uint32_t MSP_FFI_CALL msp_runtime_abi_revision(void);
MSP_FFI_API const char *MSP_FFI_CALL msp_runtime_sdk_version(void);

/*
 * Creates an empty virtual workspace. The argument is nullable. Passing a
 * workspace gives the session an owned reference for future core integration;
 * session execution itself remains limited to registered in-process commands.
 */
MSP_FFI_API MspWorkspace *MSP_FFI_CALL msp_workspace_create(void);
MSP_FFI_API void MSP_FFI_CALL msp_workspace_free(MspWorkspace *workspace);
MSP_FFI_API void MSP_FFI_CALL msp_workspace_destroy(MspWorkspace *workspace);

/* Bounded virtual-only setup helpers for the in-memory workspace. */
MSP_FFI_API int32_t MSP_FFI_CALL msp_workspace_put_file(
    MspWorkspace *workspace,
    const char *virtual_path,
    const uint8_t *data,
    size_t data_len);
MSP_FFI_API int32_t MSP_FFI_CALL msp_workspace_create_directory(
    MspWorkspace *workspace,
    const char *virtual_path);
MSP_FFI_API int32_t MSP_FFI_CALL msp_workspace_add_file(
    MspWorkspace *workspace,
    const char *virtual_path,
    const uint8_t *data,
    size_t data_len);

/* A null workspace is allowed and creates a session with no mounted workspace. */
MSP_FFI_API MspSession *MSP_FFI_CALL msp_session_create(MspWorkspace *workspace);
MSP_FFI_API MspSession *MSP_FFI_CALL msp_session_create_default(void);
MSP_FFI_API void MSP_FFI_CALL msp_session_free(MspSession *session);
MSP_FFI_API void MSP_FFI_CALL msp_session_destroy(MspSession *session);

/* command_utf8 is borrowed for the duration of the call and is bounded. */
MSP_FFI_API MspResult *MSP_FFI_CALL msp_session_run(
    MspSession *session,
    const char *command_utf8);
/* Explicit-length form; bytes must be valid UTF-8 and contain no NUL. */
MSP_FFI_API MspResult *MSP_FFI_CALL msp_session_run_n(
    MspSession *session,
    const uint8_t *command_ptr,
    size_t command_len);

MSP_FFI_API int32_t MSP_FFI_CALL msp_result_exit_code(const MspResult *result);
MSP_FFI_API const uint8_t *MSP_FFI_CALL msp_result_stdout_data(
    const MspResult *result,
    size_t *length_out);
MSP_FFI_API const uint8_t *MSP_FFI_CALL msp_result_stderr_data(
    const MspResult *result,
    size_t *length_out);
MSP_FFI_API void MSP_FFI_CALL msp_result_free(MspResult *result);

/* Short accessor spellings retained as part of the same revision. */
MSP_FFI_API int32_t MSP_FFI_CALL msp_result_exit(const MspResult *result);
MSP_FFI_API const uint8_t *MSP_FFI_CALL msp_result_stdout(
    const MspResult *result,
    size_t *length_out);
MSP_FFI_API const uint8_t *MSP_FFI_CALL msp_result_stderr(
    const MspResult *result,
    size_t *length_out);
MSP_FFI_API void MSP_FFI_CALL msp_result_destroy(MspResult *result);

/* All paths below are virtual paths. Results are owned and freed with msp_result_free. */
MSP_FFI_API MspResult *MSP_FFI_CALL msp_workspace_stat(
    MspWorkspace *workspace,
    const char *virtual_path);
MSP_FFI_API MspResult *MSP_FFI_CALL msp_workspace_list(
    MspWorkspace *workspace,
    const char *virtual_path);
MSP_FFI_API MspResult *MSP_FFI_CALL msp_workspace_list_directory(
    MspWorkspace *workspace,
    const char *virtual_path);
MSP_FFI_API MspResult *MSP_FFI_CALL msp_workspace_read(
    MspWorkspace *workspace,
    const char *virtual_path,
    uint64_t offset,
    size_t length);
MSP_FFI_API MspResult *MSP_FFI_CALL msp_workspace_read_file_range(
    MspWorkspace *workspace,
    const char *virtual_path,
    uint64_t offset,
    size_t length);

#ifdef __cplusplus
}
#endif

#endif
