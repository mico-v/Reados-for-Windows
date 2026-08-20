#include <msp_ffi.hpp>

#include <array>
#include <cstddef>
#include <cstdint>
#include <exception>
#include <initializer_list>
#include <iostream>
#include <span>
#include <string>
#include <string_view>
#include <stdexcept>
#include <type_traits>
#include <utility>
#include <vector>

namespace {

void require(bool condition, const char* message) {
    if (!condition) {
        throw std::runtime_error(message);
    }
}

void require_bytes(
    const std::vector<std::uint8_t>& actual,
    std::initializer_list<std::uint8_t> expected,
    const char* message) {
    require(
        actual == std::vector<std::uint8_t>(expected),
        message);
}

}  // namespace

int main() {
    static_assert(!std::is_copy_constructible_v<msp_ffi::Workspace>);
    static_assert(!std::is_copy_assignable_v<msp_ffi::Workspace>);
    static_assert(std::is_nothrow_move_constructible_v<msp_ffi::Workspace>);
    static_assert(!std::is_copy_constructible_v<msp_ffi::Session>);
    static_assert(!std::is_copy_assignable_v<msp_ffi::Session>);
    static_assert(std::is_nothrow_move_constructible_v<msp_ffi::Session>);
    static_assert(!std::is_copy_constructible_v<msp_ffi::Result>);
    static_assert(!std::is_copy_assignable_v<msp_ffi::Result>);
    static_assert(std::is_nothrow_move_constructible_v<msp_ffi::Result>);

    try {
        require(msp_runtime_abi_version() == MSP_FFI_ABI_VERSION, "unexpected C ABI version");
        const char* runtime_version = msp_runtime_version();
        require(runtime_version != nullptr, "runtime version is null");
        require(
            std::string_view(runtime_version) == MSP_FFI_HEADER_VERSION_STRING,
            "unexpected runtime version");

        auto workspace = msp_ffi::Workspace::create();
        const std::array<std::uint8_t, 4> binary{{0x00, 0xff, 0x41, 0x0a}};
        require(
            workspace.put_file("/bytes.bin", std::span<const std::uint8_t>(binary)) ==
                MSP_FFI_STATUS_OK,
            "put_file failed");
        require(
            workspace.create_directory("/docs") == MSP_FFI_STATUS_OK,
            "create_directory failed");
        require(
            workspace.add_file("/docs/name.bin", std::span<const std::uint8_t>(binary)) ==
                MSP_FFI_STATUS_OK,
            "add_file failed");

        const auto read = workspace.read_file_range("/bytes.bin", 0, binary.size());
        require(read.ok(), "binary read failed");
        require_bytes(read.stdout_bytes(), {0x00, 0xff, 0x41, 0x0a}, "binary bytes changed");
        require(read.stderr_bytes().empty(), "successful read wrote stderr");

        const auto stat = workspace.stat("/bytes.bin");
        require(stat.ok(), "stat failed");
        const auto stat_bytes = stat.stdout_bytes();
        const std::string stat_json(
            reinterpret_cast<const char*>(stat_bytes.data()), stat_bytes.size());
        require(stat_json.find("\"regularFile\"") != std::string::npos, "stat was not virtual");

        const auto listing = workspace.list_directory("/");
        require(listing.ok(), "list failed");
        const auto listing_bytes = listing.stdout_bytes();
        const std::string listing_json(
            reinterpret_cast<const char*>(listing_bytes.data()), listing_bytes.size());
        require(listing_json.find("bytes.bin") != std::string::npos, "list omitted virtual file");

        // A path with an embedded NUL must not be truncated to its prefix.
        const std::string embedded_path("/bad\0tail", 9);
        require(
            workspace.create_directory(embedded_path) == MSP_FFI_STATUS_INVALID_ARGUMENT,
            "embedded-NUL path was accepted");
        const std::string invalid_utf8_path(1, static_cast<char>(0xff));
        require(
            workspace.create_directory(invalid_utf8_path) == MSP_FFI_STATUS_INVALID_ARGUMENT,
            "invalid UTF-8 path was accepted");
        require(
            workspace.create_directory("C:\\secret") != MSP_FFI_STATUS_OK,
            "host-style path was accepted");

        auto session = workspace.create_session();
        const auto command = session.run("echo cpp-consumer");
        require(command.ok(), "session command failed");
        require_bytes(
            command.stdout_bytes(),
            {'c', 'p', 'p', '-', 'c', 'o', 'n', 's', 'u', 'm', 'e', 'r', '\n'},
            "command output changed");

        const std::string embedded_command("echo\0hidden", 11);
        const auto embedded_result = session.run(embedded_command);
        require(
            embedded_result.exit_code() == MSP_FFI_STATUS_INVALID_ARGUMENT,
            "embedded-NUL command was accepted");
        require(!embedded_result.stderr_bytes().empty(), "NUL rejection omitted diagnostics");

        const std::array<std::uint8_t, 1> invalid_utf8{{0xff}};
        const auto invalid_result = session.run_bytes(std::span<const std::uint8_t>(invalid_utf8));
        require(
            invalid_result.exit_code() == MSP_FFI_STATUS_INVALID_ARGUMENT,
            "invalid UTF-8 command was accepted");

        // Session creation retains its workspace reference, so explicit workspace
        // release is safe before the session is released.
        workspace.free();
        require(workspace.closed(), "workspace free did not close the owner");
        require(session.run("echo retained").ok(), "session lost retained workspace reference");
        session.close();
        require(session.closed(), "session close did not close the owner");

        auto moved = msp_ffi::Workspace::create();
        auto moved_again = std::move(moved);
        require(moved.closed(), "move did not empty the source workspace");
        moved_again.close();
        require(moved_again.closed(), "explicit close was not idempotent");

        return 0;
    } catch (const std::exception& error) {
        std::cerr << "msp_ffi C++ smoke failed: " << error.what() << '\n';
        return 1;
    }
}
