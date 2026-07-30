#pragma once

#include <stdint.h>

// ABI v2 uses only fixed-width integers and pointers across the C boundary.
// No Rust String, Vec, enum, bool, or allocator-owned container crosses it.
typedef struct MspAbiInfoV2 {
    uint32_t struct_size;
    uint32_t abi_major;
    uint32_t abi_minor;
    uint32_t reserved;
    uint64_t contract_id;
    uint64_t capabilities;
} MspAbiInfoV2;

#define MSP_ABI_V2_INFO_SIZE UINT32_C(32)
#define MSP_ABI_V2_MAJOR UINT32_C(2)
#define MSP_ABI_V2_MINOR UINT32_C(0)
#define MSP_ABI_V2_CONTRACT_ID UINT64_C(0x324D534F44414552)
#define MSP_ABI_V2_MAX_REQUEST_BYTES UINT64_C(16777216)
#define MSP_ABI_V2_MAX_RESPONSE_BYTES UINT64_C(67108864)
#define MSP_ABI_V2_MAX_PARSE_REQUEST_BYTES UINT64_C(131072)
#define MSP_ABI_V2_MAX_EXECUTE_REQUEST_BYTES UINT64_C(1048576)
#define MSP_ABI_V2_MAX_NORMALIZE_REQUEST_BYTES UINT64_C(1048576)
#define MSP_ABI_V2_MAX_PARSE_RESPONSE_BYTES UINT64_C(16777216)
#define MSP_ABI_V2_MAX_EXECUTE_RESPONSE_BYTES UINT64_C(67108864)
#define MSP_ABI_V2_MAX_NORMALIZE_RESPONSE_BYTES UINT64_C(1048576)

#define MSP_ABI_V2_CAP_LENGTH_DELIMITED_JSON (UINT64_C(1) << 0)
#define MSP_ABI_V2_CAP_EXECUTE (UINT64_C(1) << 1)
#define MSP_ABI_V2_CAP_PARSE (UINT64_C(1) << 2)
#define MSP_ABI_V2_CAP_NORMALIZE (UINT64_C(1) << 3)
#define MSP_ABI_V2_REQUIRED_CAPABILITIES UINT64_C(0xF)

#define MSP_ABI_V2_OPERATION_EXECUTE UINT32_C(1)
#define MSP_ABI_V2_OPERATION_PARSE UINT32_C(2)
#define MSP_ABI_V2_OPERATION_NORMALIZE UINT32_C(3)

#define MSP_ABI_V2_STATUS_OK INT32_C(0)
#define MSP_ABI_V2_STATUS_INVALID_ARGUMENT INT32_C(1)
#define MSP_ABI_V2_STATUS_UNSUPPORTED_OPERATION INT32_C(2)
#define MSP_ABI_V2_STATUS_REQUEST_TOO_LARGE INT32_C(3)
#define MSP_ABI_V2_STATUS_RESPONSE_TOO_LARGE INT32_C(4)
#define MSP_ABI_V2_STATUS_PANIC INT32_C(5)

#ifdef __cplusplus
extern "C" {
#endif

// Executes a JSON MSP command request and returns a heap-allocated JSON command
// result. request_json must be a valid null-terminated UTF-8 string. Execute
// requests may contain the optional internal-only `workspaceRoot` field to
// authorize a fixed-drive NTFS read-only workspace. That host path is never
// returned in result/audit JSON. The caller must release the returned pointer
// with msp_free_string. Rust panics are converted to JSON failures.
char* msp_execute_json(const char* request_json);

// Parses shell text into the internal MSP AST JSON contract.
char* msp_parse_json(const char* request_json);

// Normalizes a virtual WorkspaceFS path. The result never includes a host path.
char* msp_normalize_workspace_path_json(const char* request_json);

// Releases strings returned by msp_execute_json. value must be a pointer
// returned by an MSP JSON function and must not be freed more than once.
void msp_free_string(char* value);

// Writes the exact 32-byte ABI v2 handshake structure. out_info_size must be
// MSP_ABI_V2_INFO_SIZE exactly. A null output or any other size returns
// MSP_ABI_V2_STATUS_INVALID_ARGUMENT without writing to out_info.
int32_t msp_get_abi_info_v2(MspAbiInfoV2* out_info, uint32_t out_info_size);

// Invokes execute/parse/normalize with a length-delimited UTF-8 JSON request.
// request_ptr may be null only when request_len is zero; that combination is
// treated as an empty request and returns contract-shaped invalid-request JSON.
// Requests need no NUL terminator, and embedded NUL bytes are not truncated.
// The absolute hard request/response caps are 16 MiB and 64 MiB. Before the
// request pointer is read, operation-specific request caps are also enforced:
// Parse 128 KiB, Execute 1 MiB, Normalize 1 MiB. Responses are serialized
// through a bounded writer that stops before exceeding the operation cap:
// Parse 16 MiB, Execute 64 MiB, Normalize 1 MiB.
//
// Both output arguments must be non-null. If either is null, the other is not
// touched. Otherwise both are initialized to null/zero before later validation
// and remain null/zero for every nonzero status. On success, release the
// returned Rust-owned buffer exactly once with msp_free_buffer_v2 and the exact
// returned length. Never mix v1 and v2 free functions.
int32_t msp_invoke_v2(uint32_t operation,
                      const uint8_t* request_ptr,
                      uint64_t request_len,
                      uint8_t** out_response_ptr,
                      uint64_t* out_response_len);

// Releases a successful v2 response. ptr==NULL is always a safe no-op for any
// len. A non-null ptr must be paired with its original exact length and must not
// already have been freed.
void msp_free_buffer_v2(uint8_t* ptr, uint64_t len);

#ifdef __cplusplus
}
#endif
