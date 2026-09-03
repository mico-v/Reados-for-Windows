#include <jni.h>

#include "msp_command_runtime_ffi.h"

#include <cstdint>
#include <limits>
#include <vector>

namespace {

constexpr const char *kByteBufferClass =
    "com/reados/msp/commandruntime/MspFfiByteBuffer";

void throw_java(JNIEnv *env, const char *class_name, const char *message) {
    if (!env->ExceptionCheck()) {
        jclass exception_class = env->FindClass(class_name);
        if (exception_class != nullptr) {
            env->ThrowNew(exception_class, message);
            env->DeleteLocalRef(exception_class);
        }
    }
}

bool copy_array(JNIEnv *env, jbyteArray array, jint length,
                std::vector<uint8_t> &destination) {
    if (length < 0) {
        throw_java(env, "java/lang/IllegalArgumentException",
                   "explicit byte length must be non-negative");
        return false;
    }
    if (array == nullptr) {
        if (length != 0) {
            throw_java(env, "java/lang/IllegalArgumentException",
                       "null byte array is valid only with length zero");
            return false;
        }
        destination.clear();
        return true;
    }

    const jsize available = env->GetArrayLength(array);
    if (length > available) {
        throw_java(env, "java/lang/IllegalArgumentException",
                   "explicit byte length exceeds byte-array size");
        return false;
    }
    destination.resize(static_cast<size_t>(length));
    if (length != 0) {
        env->GetByteArrayRegion(array, 0, length,
                                reinterpret_cast<jbyte *>(destination.data()));
        if (env->ExceptionCheck()) {
            return false;
        }
    }
    return true;
}

jobject make_byte_buffer(JNIEnv *env, const uint8_t *data, size_t length) {
    if (length > static_cast<size_t>(std::numeric_limits<jint>::max())) {
        throw_java(env, "java/lang/IllegalStateException",
                   "native byte buffer exceeds the JVM array limit");
        return nullptr;
    }

    jbyteArray bytes = env->NewByteArray(static_cast<jsize>(length));
    if (bytes == nullptr) {
        return nullptr;
    }
    if (length != 0) {
        if (data == nullptr) {
            env->DeleteLocalRef(bytes);
            throw_java(env, "java/lang/IllegalStateException",
                       "native returned a null pointer for non-empty data");
            return nullptr;
        }
        env->SetByteArrayRegion(bytes, 0, static_cast<jsize>(length),
                                reinterpret_cast<const jbyte *>(data));
        if (env->ExceptionCheck()) {
            env->DeleteLocalRef(bytes);
            return nullptr;
        }
    }

    jclass buffer_class = env->FindClass(kByteBufferClass);
    if (buffer_class == nullptr) {
        env->DeleteLocalRef(bytes);
        return nullptr;
    }
    jmethodID constructor = env->GetMethodID(buffer_class, "<init>", "([BI)V");
    if (constructor == nullptr) {
        env->DeleteLocalRef(buffer_class);
        env->DeleteLocalRef(bytes);
        return nullptr;
    }
    jobject buffer = env->NewObject(buffer_class, constructor, bytes,
                                    static_cast<jint>(length));
    env->DeleteLocalRef(buffer_class);
    env->DeleteLocalRef(bytes);
    return buffer;
}

template <typename DataFunction>
jobject result_data(JNIEnv *env, jlong result_handle, DataFunction function) {
    auto *result = reinterpret_cast<const MspCommandRuntimeFfiResult *>(
        static_cast<uintptr_t>(result_handle));
    size_t length = 0;
    const uint8_t *data = function(result, &length);
    if (env->ExceptionCheck()) {
        return nullptr;
    }
    return make_byte_buffer(env, data, length);
}

} // namespace

extern "C" JNIEXPORT jint JNICALL
Java_com_reados_msp_commandruntime_MspCommandRuntimeFfiNative_nativeAbiVersion(
    JNIEnv *, jobject) {
    return static_cast<jint>(msp_command_runtime_ffi_abi_version());
}

extern "C" JNIEXPORT jobject JNICALL
Java_com_reados_msp_commandruntime_MspCommandRuntimeFfiNative_nativeVersionData(
    JNIEnv *env, jobject) {
    size_t length = 0;
    const uint8_t *data = msp_command_runtime_ffi_version_data(&length);
    return make_byte_buffer(env, data, length);
}

extern "C" JNIEXPORT jlong JNICALL
Java_com_reados_msp_commandruntime_MspCommandRuntimeFfiNative_nativeRuntimeCreate(
    JNIEnv *, jobject) {
    auto *runtime = msp_command_runtime_ffi_runtime_create();
    return static_cast<jlong>(reinterpret_cast<uintptr_t>(runtime));
}

extern "C" JNIEXPORT void JNICALL
Java_com_reados_msp_commandruntime_MspCommandRuntimeFfiNative_nativeRuntimeFree(
    JNIEnv *, jobject, jlong runtime_handle) {
    auto *runtime = reinterpret_cast<MspCommandRuntimeFfiRuntime *>(
        static_cast<uintptr_t>(runtime_handle));
    msp_command_runtime_ffi_runtime_free(runtime);
}

extern "C" JNIEXPORT jlong JNICALL
Java_com_reados_msp_commandruntime_MspCommandRuntimeFfiNative_nativeWorkspaceCreate(
    JNIEnv *, jobject) {
    auto *workspace = msp_command_runtime_ffi_workspace_create();
    return static_cast<jlong>(reinterpret_cast<uintptr_t>(workspace));
}

extern "C" JNIEXPORT void JNICALL
Java_com_reados_msp_commandruntime_MspCommandRuntimeFfiNative_nativeWorkspaceFree(
    JNIEnv *, jobject, jlong workspace_handle) {
    auto *workspace = reinterpret_cast<MspCommandRuntimeFfiWorkspace *>(
        static_cast<uintptr_t>(workspace_handle));
    msp_command_runtime_ffi_workspace_free(workspace);
}

extern "C" JNIEXPORT jint JNICALL
Java_com_reados_msp_commandruntime_MspCommandRuntimeFfiNative_nativeWorkspacePutFile(
    JNIEnv *env, jobject, jlong workspace_handle, jbyteArray path,
    jint path_length, jbyteArray data, jint data_length) {
    std::vector<uint8_t> path_copy;
    std::vector<uint8_t> data_copy;
    if (!copy_array(env, path, path_length, path_copy) ||
        !copy_array(env, data, data_length, data_copy)) {
        return 1;
    }
    auto *workspace = reinterpret_cast<MspCommandRuntimeFfiWorkspace *>(
        static_cast<uintptr_t>(workspace_handle));
    return static_cast<jint>(msp_command_runtime_ffi_workspace_put_file(
        workspace, path_copy.empty() ? nullptr : path_copy.data(),
        path_copy.size(), data_copy.empty() ? nullptr : data_copy.data(),
        data_copy.size()));
}

extern "C" JNIEXPORT jlong JNICALL
Java_com_reados_msp_commandruntime_MspCommandRuntimeFfiNative_nativeExecuteJson(
    JNIEnv *env, jobject, jlong runtime_handle, jlong workspace_handle,
    jbyteArray request, jint request_length) {
    std::vector<uint8_t> request_copy;
    if (!copy_array(env, request, request_length, request_copy)) {
        return 0;
    }
    auto *runtime = reinterpret_cast<const MspCommandRuntimeFfiRuntime *>(
        static_cast<uintptr_t>(runtime_handle));
    auto *workspace = reinterpret_cast<const MspCommandRuntimeFfiWorkspace *>(
        static_cast<uintptr_t>(workspace_handle));
    auto *result = msp_command_runtime_ffi_execute_json(
        runtime, workspace, request_copy.empty() ? nullptr : request_copy.data(),
        request_copy.size());
    return static_cast<jlong>(reinterpret_cast<uintptr_t>(result));
}

extern "C" JNIEXPORT jint JNICALL
Java_com_reados_msp_commandruntime_MspCommandRuntimeFfiNative_nativeResultExitCode(
    JNIEnv *, jobject, jlong result_handle) {
    auto *result = reinterpret_cast<const MspCommandRuntimeFfiResult *>(
        static_cast<uintptr_t>(result_handle));
    return static_cast<jint>(msp_command_runtime_ffi_result_exit_code(result));
}

extern "C" JNIEXPORT jobject JNICALL
Java_com_reados_msp_commandruntime_MspCommandRuntimeFfiNative_nativeResultStdoutData(
    JNIEnv *env, jobject, jlong result_handle) {
    return result_data(env, result_handle,
                       msp_command_runtime_ffi_result_stdout_data);
}

extern "C" JNIEXPORT jobject JNICALL
Java_com_reados_msp_commandruntime_MspCommandRuntimeFfiNative_nativeResultStderrData(
    JNIEnv *env, jobject, jlong result_handle) {
    return result_data(env, result_handle,
                       msp_command_runtime_ffi_result_stderr_data);
}

extern "C" JNIEXPORT jobject JNICALL
Java_com_reados_msp_commandruntime_MspCommandRuntimeFfiNative_nativeResultDiagnosticData(
    JNIEnv *env, jobject, jlong result_handle) {
    return result_data(env, result_handle,
                       msp_command_runtime_ffi_result_diagnostic_data);
}

extern "C" JNIEXPORT void JNICALL
Java_com_reados_msp_commandruntime_MspCommandRuntimeFfiNative_nativeResultFree(
    JNIEnv *, jobject, jlong result_handle) {
    auto *result = reinterpret_cast<MspCommandRuntimeFfiResult *>(
        static_cast<uintptr_t>(result_handle));
    msp_command_runtime_ffi_result_free(result);
}
