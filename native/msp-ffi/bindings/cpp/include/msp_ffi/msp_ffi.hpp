#pragma once

#include "msp_ffi.h"

#include <cstddef>
#include <cstdint>
#include <span>
#include <stdexcept>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

namespace msp_ffi {

/** An error raised when the C ABI cannot return an owned handle. */
class Error : public std::runtime_error {
public:
    using std::runtime_error::runtime_error;
};

namespace detail {

/*
 * The C path functions intentionally use the ABI's bounded C-string form.
 * Keep the original bytes, append a terminator only for valid C strings, and
 * pass nullptr when a caller supplied an embedded NUL. The Rust boundary then
 * rejects that input instead of silently accepting the prefix before the NUL.
 */
class CInput final {
public:
    explicit CInput(std::string_view value)
        : embedded_nul_(value.find('\0') != std::string_view::npos), storage_(value) {
        storage_.push_back('\0');
    }

    [[nodiscard]] const char* data() const noexcept {
        return embedded_nul_ ? nullptr : storage_.c_str();
    }

private:
    bool embedded_nul_;
    std::string storage_;
};

[[nodiscard]] inline std::span<const std::uint8_t> bytes_from(
    const void* data,
    std::size_t length) noexcept {
    if (length == 0) {
        return {};
    }
    return {
        reinterpret_cast<const std::uint8_t*>(data),
        length};
}

[[nodiscard]] inline const std::uint8_t* data_or_null(
    std::span<const std::uint8_t> bytes) noexcept {
    return bytes.empty() ? nullptr : bytes.data();
}

[[nodiscard]] inline std::span<const std::uint8_t> borrowed_bytes(
    const std::uint8_t* data,
    std::size_t length) noexcept {
    if (length == 0 || data == nullptr) {
        return {};
    }
    return {data, length};
}

}  // namespace detail

class Workspace;

/**
 * Move-only owner for an MspResult returned by the public C ABI.
 *
 * The view accessors borrow the C-owned bytes until this object is closed or
 * destroyed. The *_bytes accessors copy those bytes without text decoding, so
 * embedded NULs and arbitrary non-UTF-8 bytes are preserved exactly.
 */
class Result final {
public:
    Result() noexcept = default;
    ~Result() { close(); }

    Result(const Result&) = delete;
    Result& operator=(const Result&) = delete;

    Result(Result&& other) noexcept
        : handle_(std::exchange(other.handle_, nullptr)) {}

    Result& operator=(Result&& other) noexcept {
        if (this != &other) {
            close();
            handle_ = std::exchange(other.handle_, nullptr);
        }
        return *this;
    }

    /** Release the native result. Calling close repeatedly is safe. */
    void close() noexcept {
        if (handle_ != nullptr) {
            msp_result_free(handle_);
            handle_ = nullptr;
        }
    }

    /** Compatibility spelling for callers that name the C operation directly. */
    void free() noexcept { close(); }

    [[nodiscard]] bool closed() const noexcept { return handle_ == nullptr; }
    [[nodiscard]] explicit operator bool() const noexcept { return !closed(); }

    [[nodiscard]] std::int32_t exit_code() const noexcept {
        return handle_ == nullptr ? MSP_FFI_STATUS_INVALID_ARGUMENT
                                  : msp_result_exit_code(handle_);
    }

    [[nodiscard]] bool ok() const noexcept { return exit_code() == MSP_FFI_STATUS_OK; }

    /** Borrow stdout bytes until this Result is closed or destroyed. */
    [[nodiscard]] std::span<const std::uint8_t> stdout_view() const noexcept {
        if (handle_ == nullptr) {
            return {};
        }
        std::size_t length = 0;
        const auto* data = msp_result_stdout_data(handle_, &length);
        return detail::borrowed_bytes(data, length);
    }

    /** Borrow stderr bytes until this Result is closed or destroyed. */
    [[nodiscard]] std::span<const std::uint8_t> stderr_view() const noexcept {
        if (handle_ == nullptr) {
            return {};
        }
        std::size_t length = 0;
        const auto* data = msp_result_stderr_data(handle_, &length);
        return detail::borrowed_bytes(data, length);
    }

    /** Copy stdout without assuming UTF-8 or NUL termination. */
    [[nodiscard]] std::vector<std::uint8_t> stdout_bytes() const {
        const auto bytes = stdout_view();
        if (bytes.empty()) {
            return {};
        }
        return {bytes.begin(), bytes.end()};
    }

    /** Copy stderr without assuming UTF-8 or NUL termination. */
    [[nodiscard]] std::vector<std::uint8_t> stderr_bytes() const {
        const auto bytes = stderr_view();
        if (bytes.empty()) {
            return {};
        }
        return {bytes.begin(), bytes.end()};
    }

    [[nodiscard]] const std::uint8_t* stdout_data(std::size_t& length) const noexcept {
        if (handle_ == nullptr) {
            length = 0;
            return nullptr;
        }
        return msp_result_stdout_data(handle_, &length);
    }

    [[nodiscard]] const std::uint8_t* stderr_data(std::size_t& length) const noexcept {
        if (handle_ == nullptr) {
            length = 0;
            return nullptr;
        }
        return msp_result_stderr_data(handle_, &length);
    }

private:
    explicit Result(MspResult* handle) : handle_(handle) {
        if (handle_ == nullptr) {
            throw Error("msp ffi returned a null result handle");
        }
    }

    [[nodiscard]] static Result from_c(MspResult* handle) {
        return Result(handle);
    }

    MspResult* handle_ = nullptr;

    friend class Session;
    friend class Workspace;
};

/** Move-only owner for an MspSession. */
class Session final {
public:
    Session() noexcept = default;
    ~Session() { close(); }

    Session(const Session&) = delete;
    Session& operator=(const Session&) = delete;

    Session(Session&& other) noexcept
        : handle_(std::exchange(other.handle_, nullptr)) {}

    Session& operator=(Session&& other) noexcept {
        if (this != &other) {
            close();
            handle_ = std::exchange(other.handle_, nullptr);
        }
        return *this;
    }

    [[nodiscard]] static Session create() {
        return Session(msp_session_create_default());
    }

    [[nodiscard]] static Session create_default() { return create(); }

    [[nodiscard]] static Session create(const Workspace& workspace);

    /** Release the native session. Calling close repeatedly is safe. */
    void close() noexcept {
        if (handle_ != nullptr) {
            msp_session_free(handle_);
            handle_ = nullptr;
        }
    }

    /** Compatibility spelling for callers that name the C operation directly. */
    void free() noexcept { close(); }

    [[nodiscard]] bool closed() const noexcept { return handle_ == nullptr; }
    [[nodiscard]] explicit operator bool() const noexcept { return !closed(); }

    /**
     * Run bytes through msp_session_run_n so embedded NUL and invalid UTF-8
     * are rejected by the C ABI rather than truncated by a C++ c_str() call.
     */
    [[nodiscard]] Result run(std::string_view command) const {
        return run_bytes(detail::bytes_from(command.data(), command.size()));
    }

    [[nodiscard]] Result run_bytes(std::span<const std::uint8_t> command) const {
        return Result::from_c(msp_session_run_n(
            handle_, detail::data_or_null(command), command.size()));
    }

    [[nodiscard]] Result run_bytes(std::span<const std::byte> command) const {
        return run_bytes(detail::bytes_from(command.data(), command.size()));
    }

private:
    explicit Session(MspSession* handle) : handle_(handle) {
        if (handle_ == nullptr) {
            throw Error("msp ffi returned a null session handle");
        }
    }

    MspSession* handle_ = nullptr;
};

/** Move-only owner for an MspWorkspace containing only virtual paths. */
class Workspace final {
public:
    Workspace() : handle_(msp_workspace_create()) {
        if (handle_ == nullptr) {
            throw Error("msp ffi returned a null workspace handle");
        }
    }

    ~Workspace() { close(); }

    Workspace(const Workspace&) = delete;
    Workspace& operator=(const Workspace&) = delete;

    Workspace(Workspace&& other) noexcept
        : handle_(std::exchange(other.handle_, nullptr)) {}

    Workspace& operator=(Workspace&& other) noexcept {
        if (this != &other) {
            close();
            handle_ = std::exchange(other.handle_, nullptr);
        }
        return *this;
    }

    [[nodiscard]] static Workspace create() { return Workspace(); }

    /** Release the native workspace. Calling close repeatedly is safe. */
    void close() noexcept {
        if (handle_ != nullptr) {
            msp_workspace_free(handle_);
            handle_ = nullptr;
        }
    }

    /** Compatibility spelling for callers that name the C operation directly. */
    void free() noexcept { close(); }

    [[nodiscard]] bool closed() const noexcept { return handle_ == nullptr; }
    [[nodiscard]] explicit operator bool() const noexcept { return !closed(); }

    [[nodiscard]] Session create_session() const { return Session::create(*this); }
    [[nodiscard]] Session session() const { return create_session(); }

    /** Store arbitrary bytes at a virtual path. No host path is accepted here. */
    [[nodiscard]] std::int32_t put_file(
        std::string_view virtual_path,
        std::span<const std::uint8_t> data) const {
        const detail::CInput path(virtual_path);
        return msp_workspace_put_file(
            handle_, path.data(), detail::data_or_null(data), data.size());
    }

    [[nodiscard]] std::int32_t put_file(
        std::string_view virtual_path,
        std::span<const std::byte> data) const {
        return put_file(virtual_path, detail::bytes_from(data.data(), data.size()));
    }

    [[nodiscard]] std::int32_t put_file(
        std::string_view virtual_path,
        std::string_view data) const {
        return put_file(virtual_path, detail::bytes_from(data.data(), data.size()));
    }

    [[nodiscard]] std::int32_t add_file(
        std::string_view virtual_path,
        std::span<const std::uint8_t> data) const {
        const detail::CInput path(virtual_path);
        return msp_workspace_add_file(
            handle_, path.data(), detail::data_or_null(data), data.size());
    }

    [[nodiscard]] std::int32_t add_file(
        std::string_view virtual_path,
        std::span<const std::byte> data) const {
        return add_file(virtual_path, detail::bytes_from(data.data(), data.size()));
    }

    [[nodiscard]] std::int32_t add_file(
        std::string_view virtual_path,
        std::string_view data) const {
        return add_file(virtual_path, detail::bytes_from(data.data(), data.size()));
    }

    [[nodiscard]] std::int32_t create_directory(std::string_view virtual_path) const {
        const detail::CInput path(virtual_path);
        return msp_workspace_create_directory(handle_, path.data());
    }

    [[nodiscard]] Result stat(std::string_view virtual_path) const {
        const detail::CInput path(virtual_path);
        return Result::from_c(msp_workspace_stat(handle_, path.data()));
    }

    [[nodiscard]] Result list(std::string_view virtual_path) const {
        const detail::CInput path(virtual_path);
        return Result::from_c(msp_workspace_list(handle_, path.data()));
    }

    [[nodiscard]] Result list_directory(std::string_view virtual_path) const {
        const detail::CInput path(virtual_path);
        return Result::from_c(msp_workspace_list_directory(handle_, path.data()));
    }

    [[nodiscard]] Result read(
        std::string_view virtual_path,
        std::uint64_t offset,
        std::size_t length) const {
        const detail::CInput path(virtual_path);
        return Result::from_c(msp_workspace_read(handle_, path.data(), offset, length));
    }

    [[nodiscard]] Result read_file_range(
        std::string_view virtual_path,
        std::uint64_t offset,
        std::size_t length) const {
        const detail::CInput path(virtual_path);
        return Result::from_c(
            msp_workspace_read_file_range(handle_, path.data(), offset, length));
    }

private:
    [[nodiscard]] MspWorkspace* native() const noexcept { return handle_; }

    MspWorkspace* handle_ = nullptr;

    friend class Session;
};

inline Session Session::create(const Workspace& workspace) {
    return Session(msp_session_create(workspace.native()));
}

}  // namespace msp_ffi
