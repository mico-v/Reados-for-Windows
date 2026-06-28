#pragma once

#ifdef __cplusplus
extern "C" {
#endif

// Executes a JSON MSP command request and returns a heap-allocated JSON command
// result. request_json must be a valid null-terminated UTF-8 string. The caller
// must release the returned pointer with msp_free_string.
char* msp_execute_json(const char* request_json);

// Releases strings returned by msp_execute_json. value must be a pointer
// returned by msp_execute_json and must not be freed more than once.
void msp_free_string(char* value);

#ifdef __cplusplus
}
#endif
