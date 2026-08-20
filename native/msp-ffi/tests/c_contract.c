#include "msp_ffi.h"

#include <string.h>

/* Compile-time C consumer contract. The build script compiles this translation
 * unit on every target; the Rust contract test calls the function below when
 * the object is linked into the test binary. */
int msp_ffi_c_contract_compile(void) {
    uint32_t(MSP_FFI_CALL *abi_version)(void) = msp_runtime_abi_version;
    const char *(MSP_FFI_CALL *runtime_version)(void) = msp_runtime_version;
    MspWorkspace *(MSP_FFI_CALL *workspace_create)(void) = msp_workspace_create;
    void(MSP_FFI_CALL *workspace_free)(MspWorkspace *) = msp_workspace_free;
    int32_t(MSP_FFI_CALL *workspace_put_file)(MspWorkspace *, const char *, const uint8_t *, size_t) = msp_workspace_put_file;
    MspSession *(MSP_FFI_CALL *session_create)(MspWorkspace *) = msp_session_create;
    void(MSP_FFI_CALL *session_destroy)(MspSession *) = msp_session_destroy;
    MspResult *(MSP_FFI_CALL *session_run)(MspSession *, const char *) = msp_session_run;
    MspResult *(MSP_FFI_CALL *session_run_n)(MspSession *, const uint8_t *, size_t) = msp_session_run_n;
    int32_t(MSP_FFI_CALL *exit_code)(const MspResult *) = msp_result_exit_code;
    const uint8_t *(MSP_FFI_CALL *stdout_data)(const MspResult *, size_t *) = msp_result_stdout_data;
    const uint8_t *(MSP_FFI_CALL *stderr_data)(const MspResult *, size_t *) = msp_result_stderr_data;
    void(MSP_FFI_CALL *result_free)(MspResult *) = msp_result_free;
    MspResult *(MSP_FFI_CALL *workspace_stat)(MspWorkspace *, const char *) = msp_workspace_stat;
    MspResult *(MSP_FFI_CALL *workspace_list)(MspWorkspace *, const char *) = msp_workspace_list;
    MspResult *(MSP_FFI_CALL *workspace_read)(MspWorkspace *, const char *, uint64_t, size_t) = msp_workspace_read;

    (void)abi_version;
    (void)runtime_version;
    (void)workspace_create;
    (void)workspace_free;
    (void)workspace_put_file;
    (void)session_create;
    (void)session_destroy;
    (void)session_run;
    (void)session_run_n;
    (void)exit_code;
    (void)stdout_data;
    (void)stderr_data;
    (void)result_free;
    (void)workspace_stat;
    (void)workspace_list;
    (void)workspace_read;
    return 1;
}
