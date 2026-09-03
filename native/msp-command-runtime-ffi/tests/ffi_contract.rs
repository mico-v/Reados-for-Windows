use msp_command_runtime_ffi::{
    msp_command_runtime_ffi_abi_version, msp_command_runtime_ffi_execute_json,
    msp_command_runtime_ffi_result_diagnostic_data, msp_command_runtime_ffi_result_exit_code,
    msp_command_runtime_ffi_result_free, msp_command_runtime_ffi_result_stdout_data,
    msp_command_runtime_ffi_runtime_create, msp_command_runtime_ffi_runtime_free,
    msp_command_runtime_ffi_version_data, msp_command_runtime_ffi_workspace_create,
    msp_command_runtime_ffi_workspace_free, msp_command_runtime_ffi_workspace_put_file,
};

#[test]
fn exported_rust_surface_executes_virtual_pwd() {
    unsafe {
        assert_eq!(msp_command_runtime_ffi_abi_version(), 1);
        let runtime = msp_command_runtime_ffi_runtime_create();
        let workspace = msp_command_runtime_ffi_workspace_create();
        assert!(!runtime.is_null());
        assert!(!workspace.is_null());

        let request = br#"{"version":1,"command":"pwd","cwd":"/workspace"}"#;
        let result = msp_command_runtime_ffi_execute_json(
            runtime,
            workspace,
            request.as_ptr(),
            request.len(),
        );
        assert_eq!(msp_command_runtime_ffi_result_exit_code(result), 0);
        let mut stdout_len = 0;
        let stdout = msp_command_runtime_ffi_result_stdout_data(result, &mut stdout_len);
        assert_eq!(
            std::slice::from_raw_parts(stdout, stdout_len),
            b"/workspace\n"
        );
        let mut diagnostic_len = 0;
        let diagnostic =
            msp_command_runtime_ffi_result_diagnostic_data(result, &mut diagnostic_len);
        assert_eq!(
            std::slice::from_raw_parts(diagnostic, diagnostic_len),
            br#"{"code":"msp.ok"}"#
        );

        msp_command_runtime_ffi_result_free(result);
        msp_command_runtime_ffi_workspace_free(workspace);
        msp_command_runtime_ffi_runtime_free(runtime);
    }
}

#[test]
fn exported_rust_surface_executes_virtual_find() {
    unsafe {
        let runtime = msp_command_runtime_ffi_runtime_create();
        let workspace = msp_command_runtime_ffi_workspace_create();
        assert!(!runtime.is_null());
        assert!(!workspace.is_null());

        let path = b"/workspace/note.txt";
        let data = b"note";
        assert_eq!(
            msp_command_runtime_ffi_workspace_put_file(
                workspace,
                path.as_ptr(),
                path.len(),
                data.as_ptr(),
                data.len(),
            ),
            0
        );
        let request = br#"{"version":1,"command":"find /workspace -type f -name '*.txt'","cwd":"/workspace"}"#;
        let result = msp_command_runtime_ffi_execute_json(
            runtime,
            workspace,
            request.as_ptr(),
            request.len(),
        );
        assert_eq!(msp_command_runtime_ffi_result_exit_code(result), 0);
        let mut stdout_len = 0;
        let stdout = msp_command_runtime_ffi_result_stdout_data(result, &mut stdout_len);
        assert_eq!(
            std::slice::from_raw_parts(stdout, stdout_len),
            b"/workspace/note.txt\n"
        );

        msp_command_runtime_ffi_result_free(result);
        msp_command_runtime_ffi_workspace_free(workspace);
        msp_command_runtime_ffi_runtime_free(runtime);
    }
}

#[test]
fn exported_rust_surface_executes_virtual_du() {
    unsafe {
        let runtime = msp_command_runtime_ffi_runtime_create();
        let workspace = msp_command_runtime_ffi_workspace_create();
        assert!(!runtime.is_null());
        assert!(!workspace.is_null());

        let first_path = b"/workspace/a.txt";
        let first_data = b"aa";
        assert_eq!(
            msp_command_runtime_ffi_workspace_put_file(
                workspace,
                first_path.as_ptr(),
                first_path.len(),
                first_data.as_ptr(),
                first_data.len(),
            ),
            0
        );
        let second_path = b"/workspace/sub/b.txt";
        let second_data = b"bbb";
        assert_eq!(
            msp_command_runtime_ffi_workspace_put_file(
                workspace,
                second_path.as_ptr(),
                second_path.len(),
                second_data.as_ptr(),
                second_data.len(),
            ),
            0
        );
        let request = br#"{"version":1,"command":"du -s /workspace","cwd":"/workspace"}"#;
        let result = msp_command_runtime_ffi_execute_json(
            runtime,
            workspace,
            request.as_ptr(),
            request.len(),
        );
        assert_eq!(msp_command_runtime_ffi_result_exit_code(result), 0);
        let mut stdout_len = 0;
        let stdout = msp_command_runtime_ffi_result_stdout_data(result, &mut stdout_len);
        assert_eq!(
            std::slice::from_raw_parts(stdout, stdout_len),
            b"5\t/workspace\n"
        );
        assert_eq!(
            std::slice::from_raw_parts(
                msp_command_runtime_ffi_result_diagnostic_data(result, &mut stdout_len),
                stdout_len,
            ),
            br#"{"code":"msp.ok"}"#
        );

        msp_command_runtime_ffi_result_free(result);
        msp_command_runtime_ffi_workspace_free(workspace);
        msp_command_runtime_ffi_runtime_free(runtime);
    }
}
#[test]
fn version_data_is_not_nul_terminated() {
    unsafe {
        let mut length = 0;
        let data = msp_command_runtime_ffi_version_data(&mut length);
        assert_eq!(length, 5);
        assert_eq!(std::slice::from_raw_parts(data, length), b"0.1.0");
    }
}
