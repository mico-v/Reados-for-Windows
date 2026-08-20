"use strict";

const fs = require("node:fs");
const path = require("node:path");
const { TextDecoder, TextEncoder } = require("node:util");

const ABI_VERSION = 1;
const RUNTIME_VERSION = "0.1.0";

const STATUS = Object.freeze({
  OK: 0,
  ERROR: 1,
  INVALID_ARGUMENT: 2,
  LIMIT_EXCEEDED: 3,
  PANIC: 4,
});

const LIMITS = Object.freeze({
  MAX_COMMAND_BYTES: 64 * 1024,
  MAX_PATH_BYTES: 4 * 1024,
  MAX_FILE_BYTES: 8 * 1024 * 1024,
  MAX_WORKSPACE_BYTES: 64 * 1024 * 1024,
  MAX_WORKSPACE_NODES: 65_536,
  MAX_READ_BYTES: 1024 * 1024,
  MAX_RESULT_BYTES: 8 * 1024 * 1024,
  MAX_LIST_ENTRIES: 65_536,
});

const MACHINE_FOR_ARCH = Object.freeze({
  ia32: 0x014c,
  x64: 0x8664,
  arm: 0x01c4,
  arm64: 0xaa64,
});

const ARCH_FOR_MACHINE = Object.freeze(Object.fromEntries(
  Object.entries(MACHINE_FOR_ARCH).map(([arch, machine]) => [machine, arch]),
));

const strictTextEncoder = new TextEncoder();
const strictTextDecoder = new TextDecoder("utf-8", { fatal: true });
const runtimeStates = new WeakMap();
const workspaceStates = new WeakMap();
const sessionStates = new WeakMap();
const resultStates = new WeakMap();

class MspError extends Error {
  constructor(message, code, details = {}) {
    super(message, { cause: details.cause });
    this.name = new.target.name;
    this.code = code;
    for (const [key, value] of Object.entries(details)) {
      if (key !== "cause" && value !== undefined) {
        this[key] = value;
      }
    }
  }
}

class MspArgumentError extends MspError {
  constructor(message, parameter) {
    super(message, "ERR_MSP_ARGUMENT", { parameter });
  }
}

class MspNativeError extends MspError {
  constructor(message, details = {}) {
    super(message, details.code || "ERR_MSP_NATIVE", details);
  }
}

class MspArchitectureError extends MspNativeError {
  constructor(message, details = {}) {
    super(message, { ...details, code: "ERR_MSP_ARCHITECTURE" });
  }
}

class MspPlatformError extends MspNativeError {
  constructor(message, details = {}) {
    super(message, { ...details, code: "ERR_MSP_PLATFORM" });
  }
}

class MspDisposedError extends MspError {
  constructor(owner) {
    super(`${owner} has already been closed.`, "ERR_MSP_DISPOSED", { owner });
  }
}

class MspResultError extends MspError {
  constructor(operation, exitCode, stderr) {
    super(`${operation} failed with exit code ${exitCode}.`, "ERR_MSP_RESULT", {
      operation,
      exitCode,
      stderr: Uint8Array.from(stderr),
    });
  }
}

function argument(message, parameter) {
  throw new MspArgumentError(message, parameter);
}

function nativeCall(operation, callback) {
  try {
    return callback();
  } catch (error) {
    if (error instanceof MspError) {
      throw error;
    }
    throw new MspNativeError(`msp_ffi ${operation} failed.`, {
      operation,
      cause: error,
    });
  }
}

function isNullHandle(handle) {
  return handle === null || handle === undefined || handle === 0 || handle === 0n;
}

function copyBytes(value, parameter = "bytes") {
  if (value instanceof Uint8Array) {
    return Uint8Array.from(value);
  }
  if (typeof ArrayBuffer !== "undefined" && value instanceof ArrayBuffer) {
    return Uint8Array.from(new Uint8Array(value));
  }
  if (typeof ArrayBuffer !== "undefined" && ArrayBuffer.isView(value)) {
    return Uint8Array.from(new Uint8Array(value.buffer, value.byteOffset, value.byteLength));
  }
  argument(`${parameter} must be a Uint8Array, ArrayBuffer, or ArrayBuffer view.`, parameter);
}

function ensureString(value, parameter) {
  if (typeof value !== "string") {
    argument(`${parameter} must be a string.`, parameter);
  }
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index);
    if (code >= 0xd800 && code <= 0xdbff) {
      const next = value.charCodeAt(index + 1);
      if (Number.isNaN(next) || next === undefined || next < 0xdc00 || next > 0xdfff) {
        argument(`${parameter} contains an unpaired UTF-16 surrogate.`, parameter);
      }
      index += 1;
    } else if (code >= 0xdc00 && code <= 0xdfff) {
      argument(`${parameter} contains an unpaired UTF-16 surrogate.`, parameter);
    }
  }
}

function strictStringBytes(value, maximum, parameter) {
  ensureString(value, parameter);
  if (value.includes("\0")) {
    argument(`${parameter} cannot contain an embedded NUL.`, parameter);
  }
  const bytes = strictTextEncoder.encode(value);
  if (bytes.byteLength > maximum) {
    argument(`${parameter} exceeds the ${maximum}-byte limit.`, parameter);
  }
  return bytes;
}

function strictUtf8Bytes(value, maximum, parameter) {
  const bytes = copyBytes(value, parameter);
  if (bytes.byteLength > maximum) {
    argument(`${parameter} exceeds the ${maximum}-byte limit.`, parameter);
  }
  if (bytes.includes(0)) {
    argument(`${parameter} cannot contain an embedded NUL.`, parameter);
  }
  try {
    strictTextDecoder.decode(bytes);
  } catch (error) {
    throw new MspArgumentError(`${parameter} is not valid UTF-8.`, parameter, { cause: error });
  }
  return bytes;
}

function commandBytes(value) {
  return typeof value === "string"
    ? strictStringBytes(value, LIMITS.MAX_COMMAND_BYTES, "command")
    : strictUtf8Bytes(value, LIMITS.MAX_COMMAND_BYTES, "command");
}

function fileBytes(value) {
  const bytes = copyBytes(value, "data");
  if (bytes.byteLength > LIMITS.MAX_FILE_BYTES) {
    argument(`data exceeds the ${LIMITS.MAX_FILE_BYTES}-byte limit.`, "data");
  }
  return bytes;
}

function toCString(bytes) {
  const output = Buffer.allocUnsafe(bytes.byteLength + 1);
  output.set(bytes);
  output[bytes.byteLength] = 0;
  return output;
}

function validateVirtualPath(value, parameter = "virtualPath") {
  const bytes = strictStringBytes(value, LIMITS.MAX_PATH_BYTES, parameter);
  if (!value.startsWith("/")) {
    argument(`${parameter} must be an absolute virtual POSIX path.`, parameter);
  }
  if (value.startsWith("//") || value.includes("\\") || value.includes(":")) {
    argument(`${parameter} is not a virtual POSIX path.`, parameter);
  }

  for (const component of value.split("/")) {
    if (component === "" && value !== "/") {
      // Repeated separators are safe in the native normalizer and are retained.
      continue;
    }
    if (component === "." || component === "..") {
      argument(`${parameter} cannot contain traversal components.`, parameter);
    }
    if (component.toLowerCase() === ".msp") {
      argument(`${parameter} targets a hidden workspace component.`, parameter);
    }
    if ([...component].some((character) => {
      const code = character.charCodeAt(0);
      return code < 0x20 || code === 0x7f || "<>\"|?*".includes(character);
    })) {
      argument(`${parameter} contains a host-invalid component.`, parameter);
    }
    if (component.endsWith(" ") || component.endsWith(".")) {
      argument(`${parameter} contains a host-ambiguous component.`, parameter);
    }
    const stem = component.split(".", 1)[0].replace(/[ .]+$/g, "").toUpperCase();
    if (
      ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$", "CLOCK$"].includes(stem)
      || /^COM[1-9]$/.test(stem)
      || /^LPT[1-9]$/.test(stem)
    ) {
      argument(`${parameter} contains a reserved Windows component.`, parameter);
    }
  }
  return bytes;
}

function normalizeAbsoluteDllPath(dllPath) {
  if (typeof dllPath !== "string" || dllPath.length === 0) {
    argument("dllPath must be a non-empty absolute path.", "dllPath");
  }
  if (dllPath.includes("\0")) {
    argument("dllPath cannot contain an embedded NUL.", "dllPath");
  }
  const windowsAbsolute = path.win32.isAbsolute(dllPath);
  if (!path.isAbsolute(dllPath) && !windowsAbsolute) {
    argument("dllPath must be an absolute path; relative paths are not loaded.", "dllPath");
  }
  const normalized = windowsAbsolute ? path.win32.normalize(dllPath) : path.normalize(dllPath);
  if (path.win32.extname(normalized).toLowerCase() !== ".dll") {
    argument("dllPath must name an msp_ffi DLL (.dll).", "dllPath");
  }
  return normalized;
}

function readAt(fsApi, descriptor, buffer, position) {
  let read = 0;
  while (read < buffer.length) {
    const count = fsApi.readSync(
      descriptor,
      buffer,
      read,
      buffer.length - read,
      position + read,
    );
    if (!count) {
      break;
    }
    read += count;
  }
  return read;
}

function inspectPeMachine(dllPath, fsApi = fs) {
  let descriptor;
  let stat;
  try {
    stat = fsApi.statSync(dllPath);
  } catch (error) {
    throw new MspNativeError(`The explicitly selected DLL was not found: ${dllPath}`, {
      code: "ERR_MSP_DLL_NOT_FOUND",
      path: dllPath,
      cause: error,
    });
  }
  if (!stat.isFile()) {
    throw new MspNativeError(`The explicitly selected DLL is not a regular file: ${dllPath}`, {
      code: "ERR_MSP_DLL_NOT_FILE",
      path: dllPath,
    });
  }

  try {
    descriptor = fsApi.openSync(dllPath, "r");
    const dos = Buffer.alloc(64);
    if (readAt(fsApi, descriptor, dos, 0) !== dos.length || dos.toString("ascii", 0, 2) !== "MZ") {
      throw new MspArchitectureError(`The selected file is not a valid PE DLL: ${dllPath}`, {
        path: dllPath,
      });
    }
    const peOffset = dos.readUInt32LE(0x3c);
    if (peOffset < 64 || peOffset > 16 * 1024 * 1024 || peOffset + 6 > stat.size) {
      throw new MspArchitectureError(`The selected DLL has an invalid PE header: ${dllPath}`, {
        path: dllPath,
      });
    }
    const pe = Buffer.alloc(6);
    if (readAt(fsApi, descriptor, pe, peOffset) !== pe.length || pe.toString("ascii", 0, 4) !== "PE\0\0") {
      throw new MspArchitectureError(`The selected file is not a valid PE DLL: ${dllPath}`, {
        path: dllPath,
      });
    }
    return pe.readUInt16LE(4);
  } catch (error) {
    if (error instanceof MspError) {
      throw error;
    }
    throw new MspArchitectureError(`Unable to inspect the selected DLL: ${dllPath}`, {
      path: dllPath,
      cause: error,
    });
  } finally {
    if (descriptor !== undefined) {
      try {
        fsApi.closeSync(descriptor);
      } catch {
        // Preserve the validation or load error; closing an inspection handle is best effort.
      }
    }
  }
}

function selectDll(dllPath, options = {}) {
  const fullPath = normalizeAbsoluteDllPath(dllPath);
  const platform = options.platform || process.platform;
  if (platform !== "win32") {
    throw new MspPlatformError("The msp_ffi Node loader supports Windows PE DLLs only.", {
      platform,
      path: fullPath,
    });
  }
  const arch = options.arch || process.arch;
  const expectedMachine = MACHINE_FOR_ARCH[arch];
  if (expectedMachine === undefined) {
    throw new MspArchitectureError(`Unsupported Node architecture: ${arch}.`, {
      architecture: arch,
      path: fullPath,
    });
  }
  const machine = inspectPeMachine(fullPath, options.fs || fs);
  if (machine !== expectedMachine) {
    throw new MspArchitectureError(
      `The selected DLL architecture (${ARCH_FOR_MACHINE[machine] || `0x${machine.toString(16)}`}) `
        + `does not match Node architecture (${arch}).`,
      {
        path: fullPath,
        architecture: arch,
        machine,
        expectedMachine,
      },
    );
  }
  return Object.freeze({ path: fullPath, architecture: arch, machine });
}

function assertBackend(backend) {
  const required = [
    "runtimeAbiVersion",
    "runtimeVersion",
    "workspaceCreate",
    "workspaceFree",
    "workspacePutFile",
    "workspaceAddFile",
    "workspaceCreateDirectory",
    "sessionCreate",
    "sessionCreateDefault",
    "sessionFree",
    "sessionRunN",
    "resultSnapshot",
    "resultFree",
    "workspaceStat",
    "workspaceList",
    "workspaceListDirectory",
    "workspaceRead",
    "workspaceReadFileRange",
  ];
  if (!backend || typeof backend !== "object") {
    throw new MspNativeError("The native loader backend must be an object.", {
      code: "ERR_MSP_FFI_BACKEND",
    });
  }
  for (const method of required) {
    if (typeof backend[method] !== "function") {
      throw new MspNativeError(`The native loader backend is missing ${method}().`, {
        code: "ERR_MSP_FFI_BACKEND",
        method,
      });
    }
  }
  return backend;
}

function normalizeRuntimeVersion(value) {
  if (typeof value === "string") {
    return value;
  }
  if (Buffer.isBuffer(value) || value instanceof Uint8Array) {
    return Buffer.from(value).toString("utf8").replace(/\0.*$/s, "");
  }
  return String(value);
}

function readRuntimeInfo(backend) {
  const abiVersion = nativeCall("msp_runtime_abi_version", () => backend.runtimeAbiVersion());
  const version = normalizeRuntimeVersion(nativeCall("msp_runtime_version", () => backend.runtimeVersion()));
  if (!Number.isInteger(abiVersion) || abiVersion !== ABI_VERSION || version !== RUNTIME_VERSION) {
    throw new MspNativeError(
      `Unsupported msp_ffi runtime ABI/version: ${abiVersion}/${version}; `
        + `expected ${ABI_VERSION}/${RUNTIME_VERSION}.`,
      {
        code: "ERR_MSP_ABI_MISMATCH",
        abiVersion,
        version,
        expectedAbiVersion: ABI_VERSION,
        expectedVersion: RUNTIME_VERSION,
      },
    );
  }
  return Object.freeze({ abiVersion, version });
}

function normalizeSnapshot(snapshot) {
  if (!snapshot || typeof snapshot !== "object") {
    throw new MspNativeError("The native result snapshot was not an object.", {
      code: "ERR_MSP_RESULT_SNAPSHOT",
    });
  }
  if (!Number.isInteger(snapshot.exitCode)) {
    throw new MspNativeError("The native result snapshot has an invalid exit code.", {
      code: "ERR_MSP_RESULT_SNAPSHOT",
    });
  }
  const stdout = copyBytes(snapshot.stdout || new Uint8Array(), "stdout");
  const stderr = copyBytes(snapshot.stderr || new Uint8Array(), "stderr");
  if (stdout.byteLength > LIMITS.MAX_RESULT_BYTES || stderr.byteLength > LIMITS.MAX_RESULT_BYTES) {
    throw new MspNativeError(
      `The native result exceeds the ${LIMITS.MAX_RESULT_BYTES}-byte stream limit.`,
      { code: "ERR_MSP_RESULT_LIMIT" },
    );
  }
  return {
    exitCode: snapshot.exitCode,
    stdout,
    stderr,
  };
}

function copyNativeResult(runtime, nativeResult, operation) {
  if (isNullHandle(nativeResult)) {
    throw new MspNativeError(`msp_ffi ${operation} returned a null result handle.`, {
      operation,
    });
  }
  const state = runtimeStates.get(runtime);
  let snapshot;
  let copyError;
  try {
    snapshot = normalizeSnapshot(nativeCall(`${operation}:snapshot`, () => (
      state.backend.resultSnapshot(nativeResult)
    )));
  } catch (error) {
    copyError = error;
  }
  try {
    nativeCall(`${operation}:free`, () => state.backend.resultFree(nativeResult));
  } catch (error) {
    if (!copyError) {
      copyError = error;
    }
  }
  if (copyError) {
    throw copyError;
  }
  return new MspResult(snapshot);
}

class MspResult {
  constructor(snapshot) {
    const state = {
      exitCode: snapshot.exitCode,
      stdout: Uint8Array.from(snapshot.stdout),
      stderr: Uint8Array.from(snapshot.stderr),
    };
    resultStates.set(this, state);
    Object.defineProperties(this, {
      exitCode: { enumerable: true, get: () => state.exitCode },
      status: { enumerable: true, get: () => state.exitCode },
      exit: { enumerable: true, get: () => state.exitCode },
      ok: { enumerable: true, get: () => state.exitCode === STATUS.OK },
      isSuccess: { enumerable: true, get: () => state.exitCode === STATUS.OK },
      stdout: { enumerable: true, get: () => Uint8Array.from(state.stdout) },
      stderr: { enumerable: true, get: () => Uint8Array.from(state.stderr) },
      stdoutBytes: { enumerable: true, get: () => Uint8Array.from(state.stdout) },
      stderrBytes: { enumerable: true, get: () => Uint8Array.from(state.stderr) },
    });
  }

  copyStdout() {
    return this.stdout;
  }

  copyStderr() {
    return this.stderr;
  }

  ensureSuccess(operation = "msp_ffi operation") {
    const state = resultStates.get(this);
    if (!state) {
      throw new MspDisposedError("MspResult");
    }
    if (state.exitCode !== STATUS.OK) {
      throw new MspResultError(operation, state.exitCode, state.stderr);
    }
  }

  toWorkerData() {
    return {
      exitCode: this.exitCode,
      stdout: this.stdout,
      stderr: this.stderr,
    };
  }

  toJSON() {
    return this.toWorkerData();
  }

  close() {
    // Native result ownership is released before this object is returned.
  }

  dispose() {
    this.close();
  }
}

class MspRuntime {
  constructor(backend, selection, info) {
    runtimeStates.set(this, { backend, selection, info });
  }

  get dllPath() {
    return runtimeStates.get(this).selection.path;
  }

  get architecture() {
    return runtimeStates.get(this).selection.architecture;
  }

  get machine() {
    return runtimeStates.get(this).selection.machine;
  }

  get abiVersion() {
    return runtimeStates.get(this).info.abiVersion;
  }

  get version() {
    return runtimeStates.get(this).info.version;
  }

  get info() {
    const state = runtimeStates.get(this);
    return Object.freeze({
      abiVersion: state.info.abiVersion,
      version: state.info.version,
      dllPath: state.selection.path,
      architecture: state.selection.architecture,
      machine: state.selection.machine,
    });
  }

  createWorkspace() {
    const state = runtimeStates.get(this);
    const handle = nativeCall("msp_workspace_create", () => state.backend.workspaceCreate());
    if (isNullHandle(handle)) {
      throw new MspNativeError("msp_workspace_create returned a null workspace handle.");
    }
    return new MspWorkspace(this, handle);
  }

  createDefaultSession() {
    const state = runtimeStates.get(this);
    const handle = nativeCall("msp_session_create_default", () => state.backend.sessionCreateDefault());
    if (isNullHandle(handle)) {
      throw new MspNativeError("msp_session_create_default returned a null session handle.");
    }
    return new MspSession(this, handle);
  }

  createSession(workspace) {
    if (workspace === undefined) {
      return this.createDefaultSession();
    }
    if (!(workspace instanceof MspWorkspace)) {
      argument("workspace must be an MspWorkspace.", "workspace");
    }
    return workspace.createSession();
  }

  close() {
    // Koffi owns the process-wide library handle. There is no unsafe unload API.
  }

  dispose() {
    this.close();
  }
}

function ensureWorkspaceState(workspace) {
  const state = workspaceStates.get(workspace);
  if (!state || state.closed) {
    throw new MspDisposedError("MspWorkspace");
  }
  return state;
}

function ensureSessionState(session) {
  const state = sessionStates.get(session);
  if (!state || state.closed) {
    throw new MspDisposedError("MspSession");
  }
  return state;
}

function statusCall(state, operation, callback) {
  return nativeCall(operation, () => {
    const status = callback();
    if (!Number.isInteger(status)) {
      throw new MspNativeError(`msp_ffi ${operation} returned a non-integer status.`, {
        operation,
      });
    }
    return status;
  });
}

function normalizeOffset(offset) {
  if (typeof offset === "bigint") {
    if (offset < 0n || offset > 0xffffffffffffffffn) {
      argument("offset must fit in an unsigned 64-bit integer.", "offset");
    }
    return offset;
  }
  if (!Number.isSafeInteger(offset) || offset < 0) {
    argument("offset must be a non-negative safe integer or bigint.", "offset");
  }
  return offset;
}

function normalizeReadLength(length) {
  if (!Number.isSafeInteger(length) || length < 0 || length > LIMITS.MAX_READ_BYTES) {
    argument(`length must be an integer between 0 and ${LIMITS.MAX_READ_BYTES}.`, "length");
  }
  return length;
}

class MspWorkspace {
  constructor(runtime, handle) {
    workspaceStates.set(this, { runtime, handle, closed: false });
  }

  get closed() {
    return workspaceStates.get(this)?.closed === true;
  }

  createSession() {
    const state = ensureWorkspaceState(this);
    const runtimeState = runtimeStates.get(state.runtime);
    const handle = nativeCall("msp_session_create", () => (
      runtimeState.backend.sessionCreate(state.handle)
    ));
    if (isNullHandle(handle)) {
      throw new MspNativeError("msp_session_create returned a null session handle.");
    }
    return new MspSession(state.runtime, handle);
  }

  putFile(virtualPath, data) {
    const state = ensureWorkspaceState(this);
    const pathBytes = validateVirtualPath(virtualPath);
    const bytes = fileBytes(data);
    const runtimeState = runtimeStates.get(state.runtime);
    return statusCall(state, "msp_workspace_put_file", () => runtimeState.backend.workspacePutFile(
      state.handle,
      toCString(pathBytes),
      bytes.length ? Buffer.from(bytes) : null,
      bytes.length,
    ));
  }

  addFile(virtualPath, data) {
    const state = ensureWorkspaceState(this);
    const pathBytes = validateVirtualPath(virtualPath);
    const bytes = fileBytes(data);
    const runtimeState = runtimeStates.get(state.runtime);
    return statusCall(state, "msp_workspace_add_file", () => runtimeState.backend.workspaceAddFile(
      state.handle,
      toCString(pathBytes),
      bytes.length ? Buffer.from(bytes) : null,
      bytes.length,
    ));
  }

  createDirectory(virtualPath) {
    const state = ensureWorkspaceState(this);
    const pathBytes = validateVirtualPath(virtualPath);
    const runtimeState = runtimeStates.get(state.runtime);
    return statusCall(state, "msp_workspace_create_directory", () => (
      runtimeState.backend.workspaceCreateDirectory(state.handle, toCString(pathBytes))
    ));
  }

  stat(virtualPath) {
    return this.#pathResult("msp_workspace_stat", "workspaceStat", virtualPath);
  }

  list(virtualPath) {
    return this.listDirectory(virtualPath);
  }

  listDirectory(virtualPath) {
    return this.#pathResult("msp_workspace_list_directory", "workspaceListDirectory", virtualPath);
  }

  read(virtualPath, offset, length) {
    return this.#readResult("msp_workspace_read", "workspaceRead", virtualPath, offset, length);
  }

  readFileRange(virtualPath, offset, length) {
    return this.#readResult(
      "msp_workspace_read_file_range",
      "workspaceReadFileRange",
      virtualPath,
      offset,
      length,
    );
  }

  close() {
    const state = workspaceStates.get(this);
    if (!state || state.closed) {
      return;
    }
    state.closed = true;
    const runtimeState = runtimeStates.get(state.runtime);
    nativeCall("msp_workspace_free", () => runtimeState.backend.workspaceFree(state.handle));
  }

  dispose() {
    this.close();
  }

  #pathResult(operation, method, virtualPath) {
    const state = ensureWorkspaceState(this);
    const pathBytes = validateVirtualPath(virtualPath);
    const runtimeState = runtimeStates.get(state.runtime);
    const nativeResult = nativeCall(operation, () => runtimeState.backend[method](
      state.handle,
      toCString(pathBytes),
    ));
    return copyNativeResult(state.runtime, nativeResult, operation);
  }

  #readResult(operation, method, virtualPath, offset, length) {
    const state = ensureWorkspaceState(this);
    const pathBytes = validateVirtualPath(virtualPath);
    const normalizedOffset = normalizeOffset(offset);
    const normalizedLength = normalizeReadLength(length);
    const runtimeState = runtimeStates.get(state.runtime);
    const nativeResult = nativeCall(operation, () => runtimeState.backend[method](
      state.handle,
      toCString(pathBytes),
      normalizedOffset,
      normalizedLength,
    ));
    return copyNativeResult(state.runtime, nativeResult, operation);
  }
}

class MspSession {
  constructor(runtime, handle) {
    sessionStates.set(this, { runtime, handle, closed: false });
  }

  get closed() {
    return sessionStates.get(this)?.closed === true;
  }

  run(command) {
    const state = ensureSessionState(this);
    const bytes = commandBytes(command);
    const runtimeState = runtimeStates.get(state.runtime);
    const nativeResult = nativeCall("msp_session_run_n", () => runtimeState.backend.sessionRunN(
      state.handle,
      bytes.length ? Buffer.from(bytes) : null,
      bytes.length,
    ));
    return copyNativeResult(state.runtime, nativeResult, "msp_session_run_n");
  }

  runBytes(command) {
    return this.run(command);
  }

  close() {
    const state = sessionStates.get(this);
    if (!state || state.closed) {
      return;
    }
    state.closed = true;
    const runtimeState = runtimeStates.get(state.runtime);
    nativeCall("msp_session_free", () => runtimeState.backend.sessionFree(state.handle));
  }

  dispose() {
    this.close();
  }
}

function optionalRequire(moduleName) {
  try {
    return require(moduleName);
  } catch (error) {
    if (error && error.code === "MODULE_NOT_FOUND") {
      return null;
    }
    throw error;
  }
}

function koffiOut() {
  // Koffi marshals primitive pointer outputs through a one-element JS array
  // when the prototype marks the pointer as _Out_.
  return [0];
}

function readKoffiOut(value) {
  if (Array.isArray(value) && value.length > 0) {
    return readKoffiOut(value[0]);
  }
  if (typeof value === "number" || typeof value === "bigint") {
    return Number(value);
  }
  if (value && (typeof value.value === "number" || typeof value.value === "bigint")) {
    return Number(value.value);
  }
  if (Buffer.isBuffer(value)) {
    if (value.length >= 8) {
      return Number(value.readBigUInt64LE(0));
    }
    if (value.length >= 4) {
      return value.readUInt32LE(0);
    }
  }
  throw new MspNativeError("Koffi returned an unreadable size_t output.", {
    code: "ERR_MSP_FFI_BACKEND",
  });
}

function copyKoffiPointer(koffi, pointer, length) {
  if (length === 0) {
    return new Uint8Array();
  }
  if (pointer === null || pointer === undefined) {
    throw new MspNativeError("msp_ffi returned a null data pointer for a non-empty result.");
  }
  if (typeof koffi.decode === "function" && typeof koffi.array === "function") {
    const decoded = koffi.decode(pointer, koffi.array("uint8_t", length));
    return copyBytes(decoded, "native result bytes");
  }
  if (Buffer.isBuffer(pointer) && pointer.length >= length) {
    return Uint8Array.from(pointer.subarray(0, length));
  }
  throw new MspNativeError("The Koffi backend cannot copy a native result buffer.", {
    code: "ERR_MSP_FFI_BACKEND",
  });
}

function createKoffiBackend(koffi, dllPath) {
  const selectedPath = normalizeAbsoluteDllPath(dllPath);
  let library;
  try {
    library = koffi.load(selectedPath);
  } catch (error) {
    throw new MspNativeError(`Unable to load the explicitly selected msp_ffi DLL: ${dllPath}`, {
      code: "ERR_MSP_DLL_LOAD",
      path: dllPath,
      cause: error,
    });
  }
  const fn = (signature) => {
    try {
      return library.func(signature);
    } catch (error) {
      throw new MspNativeError(`The selected DLL is missing an msp_ffi export: ${signature}`, {
        code: "ERR_MSP_EXPORT",
        signature,
        cause: error,
      });
    }
  };

  const functions = {
    runtimeAbiVersion: fn("uint32_t msp_runtime_abi_version(void)"),
    runtimeVersion: fn("const char * msp_runtime_version(void)"),
    workspaceCreate: fn("void * msp_workspace_create(void)"),
    workspaceFree: fn("void msp_workspace_free(void * workspace)"),
    workspacePutFile: fn("int32_t msp_workspace_put_file(void * workspace, const char * path, const uint8_t * data, size_t length)"),
    workspaceAddFile: fn("int32_t msp_workspace_add_file(void * workspace, const char * path, const uint8_t * data, size_t length)"),
    workspaceCreateDirectory: fn("int32_t msp_workspace_create_directory(void * workspace, const char * path)"),
    sessionCreate: fn("void * msp_session_create(void * workspace)"),
    sessionCreateDefault: fn("void * msp_session_create_default(void)"),
    sessionFree: fn("void msp_session_free(void * session)"),
    sessionRunN: fn("void * msp_session_run_n(void * session, const uint8_t * command, size_t length)"),
    resultExitCode: fn("int32_t msp_result_exit_code(const void * result)"),
    resultStdout: fn("const uint8_t * msp_result_stdout_data(const void * result, _Out_ size_t * length)"),
    resultStderr: fn("const uint8_t * msp_result_stderr_data(const void * result, _Out_ size_t * length)"),
    resultFree: fn("void msp_result_free(void * result)"),
    workspaceStat: fn("void * msp_workspace_stat(void * workspace, const char * path)"),
    workspaceList: fn("void * msp_workspace_list(void * workspace, const char * path)"),
    workspaceListDirectory: fn("void * msp_workspace_list_directory(void * workspace, const char * path)"),
    workspaceRead: fn("void * msp_workspace_read(void * workspace, const char * path, uint64_t offset, size_t length)"),
    workspaceReadFileRange: fn("void * msp_workspace_read_file_range(void * workspace, const char * path, uint64_t offset, size_t length)"),
  };

  function readResultData(result, accessor) {
    const outputLength = koffiOut();
    const pointer = functions[accessor](result, outputLength);
    const length = readKoffiOut(outputLength);
    if (!Number.isSafeInteger(length) || length < 0) {
      throw new MspNativeError("msp_ffi returned an invalid result length.", {
        code: "ERR_MSP_RESULT_SNAPSHOT",
      });
    }
    return copyKoffiPointer(koffi, pointer, length);
  }

  return assertBackend({
    runtimeAbiVersion: () => functions.runtimeAbiVersion(),
    runtimeVersion: () => functions.runtimeVersion(),
    workspaceCreate: () => functions.workspaceCreate(),
    workspaceFree: (workspace) => functions.workspaceFree(workspace),
    workspacePutFile: (workspace, pathBytes, data, length) => functions.workspacePutFile(workspace, pathBytes, data, length),
    workspaceAddFile: (workspace, pathBytes, data, length) => functions.workspaceAddFile(workspace, pathBytes, data, length),
    workspaceCreateDirectory: (workspace, pathBytes) => functions.workspaceCreateDirectory(workspace, pathBytes),
    sessionCreate: (workspace) => functions.sessionCreate(workspace),
    sessionCreateDefault: () => functions.sessionCreateDefault(),
    sessionFree: (session) => functions.sessionFree(session),
    sessionRunN: (session, command, length) => functions.sessionRunN(session, command, length),
    resultSnapshot: (result) => ({
      exitCode: functions.resultExitCode(result),
      stdout: readResultData(result, "resultStdout"),
      stderr: readResultData(result, "resultStderr"),
    }),
    resultFree: (result) => functions.resultFree(result),
    workspaceStat: (workspace, pathBytes) => functions.workspaceStat(workspace, pathBytes),
    workspaceList: (workspace, pathBytes) => functions.workspaceList(workspace, pathBytes),
    workspaceListDirectory: (workspace, pathBytes) => functions.workspaceListDirectory(workspace, pathBytes),
    workspaceRead: (workspace, pathBytes, offset, length) => functions.workspaceRead(workspace, pathBytes, offset, length),
    workspaceReadFileRange: (workspace, pathBytes, offset, length) => functions.workspaceReadFileRange(workspace, pathBytes, offset, length),
  });
}

function loadDefaultBackend(dllPath) {
  const koffi = optionalRequire("koffi");
  if (!koffi) {
    throw new MspNativeError(
      "No Node FFI backend is installed. Install the optional 'koffi' peer dependency "
        + "or pass an explicit backend to loadMspFfi().",
      {
        code: "ERR_MSP_FFI_BACKEND_MISSING",
        path: dllPath,
      },
    );
  }
  return createKoffiBackend(koffi, dllPath);
}

function loadMspFfi(dllPath, options = {}) {
  const selection = selectDll(dllPath, options);
  let backend;
  if (options.backend !== undefined) {
    backend = typeof options.backend === "function"
      ? options.backend(selection.path)
      : options.backend;
  } else {
    backend = loadDefaultBackend(selection.path);
  }
  assertBackend(backend);
  const info = readRuntimeInfo(backend);
  return new MspRuntime(backend, selection, info);
}

module.exports = {
  ABI_VERSION,
  RUNTIME_VERSION,
  STATUS,
  LIMITS,
  MACHINE_FOR_ARCH,
  MspError,
  MspArgumentError,
  MspNativeError,
  MspArchitectureError,
  MspPlatformError,
  MspDisposedError,
  MspResultError,
  MspRuntime,
  MspWorkspace,
  MspSession,
  MspResult,
  validateVirtualPath,
  normalizeAbsoluteDllPath,
  inspectPeMachine,
  selectDll,
  createKoffiBackend,
  loadMspFfi,
  loadNativeLibrary: loadMspFfi,
};
