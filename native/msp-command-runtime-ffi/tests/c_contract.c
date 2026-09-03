#include "../include/msp_command_runtime_ffi.h"

#include <assert.h>

int main(void) {
    uint32_t (*abi)(void) = &msp_command_runtime_ffi_abi_version;
    const uint8_t *(*version)(size_t *) = &msp_command_runtime_ffi_version_data;
    MspCommandRuntimeFfiRuntime *(*runtime_create)(void) = &msp_command_runtime_ffi_runtime_create;
    void (*runtime_free)(MspCommandRuntimeFfiRuntime *) = &msp_command_runtime_ffi_runtime_free;
    MspCommandRuntimeFfiWorkspace *(*workspace_create)(void) = &msp_command_runtime_ffi_workspace_create;
    void (*workspace_free)(MspCommandRuntimeFfiWorkspace *) = &msp_command_runtime_ffi_workspace_free;
    int32_t (*put_file)(MspCommandRuntimeFfiWorkspace *, const uint8_t *, size_t, const uint8_t *, size_t) = &msp_command_runtime_ffi_workspace_put_file;
    MspCommandRuntimeFfiResult *(*execute)(const MspCommandRuntimeFfiRuntime *, const MspCommandRuntimeFfiWorkspace *, const uint8_t *, size_t) = &msp_command_runtime_ffi_execute_json;
    int32_t (*exit_code)(const MspCommandRuntimeFfiResult *) = &msp_command_runtime_ffi_result_exit_code;
    const uint8_t *(*stdout_data)(const MspCommandRuntimeFfiResult *, size_t *) = &msp_command_runtime_ffi_result_stdout_data;
    const uint8_t *(*stderr_data)(const MspCommandRuntimeFfiResult *, size_t *) = &msp_command_runtime_ffi_result_stderr_data;
    const uint8_t *(*diagnostic_data)(const MspCommandRuntimeFfiResult *, size_t *) = &msp_command_runtime_ffi_result_diagnostic_data;
    void (*result_free)(MspCommandRuntimeFfiResult *) = &msp_command_runtime_ffi_result_free;
    assert(abi && version && runtime_create && runtime_free && workspace_create && workspace_free);
    assert(put_file && execute && exit_code && stdout_data && stderr_data && diagnostic_data && result_free);
    return 0;
}
