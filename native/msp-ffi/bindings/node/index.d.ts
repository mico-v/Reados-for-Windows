export type ByteSource = Uint8Array | ArrayBuffer | ArrayBufferView;
export type NativeArchitecture = "ia32" | "x64" | "arm" | "arm64";
export type NativeOffset = number | bigint;

export interface ResultSnapshot {
  exitCode: number;
  stdout: ByteSource;
  stderr: ByteSource;
}

/** The narrow ABI adapter used by the package loader. */
export interface NativeBackend {
  runtimeAbiVersion(): number;
  runtimeVersion(): string | Uint8Array;
  workspaceCreate(): unknown;
  workspaceFree(workspace: unknown): void;
  workspacePutFile(workspace: unknown, virtualPath: Uint8Array, data: Uint8Array | null, length: number): number;
  workspaceAddFile(workspace: unknown, virtualPath: Uint8Array, data: Uint8Array | null, length: number): number;
  workspaceCreateDirectory(workspace: unknown, virtualPath: Uint8Array): number;
  sessionCreate(workspace: unknown): unknown;
  sessionCreateDefault(): unknown;
  sessionFree(session: unknown): void;
  sessionRunN(session: unknown, command: Uint8Array | null, length: number): unknown;
  resultSnapshot(result: unknown): ResultSnapshot;
  resultFree(result: unknown): void;
  workspaceStat(workspace: unknown, virtualPath: Uint8Array): unknown;
  workspaceList(workspace: unknown, virtualPath: Uint8Array): unknown;
  workspaceListDirectory(workspace: unknown, virtualPath: Uint8Array): unknown;
  workspaceRead(workspace: unknown, virtualPath: Uint8Array, offset: NativeOffset, length: number): unknown;
  workspaceReadFileRange(workspace: unknown, virtualPath: Uint8Array, offset: NativeOffset, length: number): unknown;
}

export interface DllInspectionFs {
  statSync(path: string): { isFile(): boolean; size: number };
  openSync(path: string, flags: string): unknown;
  readSync(fd: unknown, buffer: BufferLike, offset: number, length: number, position: number): number;
  closeSync(fd: unknown): void;
}

export interface BufferLike {
  readonly length: number;
  [index: number]: number;
}

export interface LoadOptions {
  /** Offline tests and advanced hosts may provide an explicit ABI adapter. */
  backend?: NativeBackend | ((dllPath: string) => NativeBackend);
  /** Test-only platform override; production callers should not set this. */
  platform?: string;
  /** Test-only architecture override; production callers should not set this. */
  arch?: string;
  /** Test-only bounded file inspection adapter. */
  fs?: DllInspectionFs;
}

export interface DllSelection {
  readonly path: string;
  readonly architecture: string;
  readonly machine: number;
}

export interface RuntimeInfo extends DllSelection {
  readonly abiVersion: number;
  readonly version: string;
}

export const ABI_VERSION: 1;
export const RUNTIME_VERSION: "0.1.0";
export const STATUS: Readonly<{
  readonly OK: 0;
  readonly ERROR: 1;
  readonly INVALID_ARGUMENT: 2;
  readonly LIMIT_EXCEEDED: 3;
  readonly PANIC: 4;
}>;
export const LIMITS: Readonly<{
  readonly MAX_COMMAND_BYTES: 65536;
  readonly MAX_PATH_BYTES: 4096;
  readonly MAX_FILE_BYTES: 8388608;
  readonly MAX_WORKSPACE_BYTES: 67108864;
  readonly MAX_WORKSPACE_NODES: 65536;
  readonly MAX_READ_BYTES: 1048576;
  readonly MAX_RESULT_BYTES: 8388608;
  readonly MAX_LIST_ENTRIES: 65536;
}>;
export const MACHINE_FOR_ARCH: Readonly<Record<NativeArchitecture, number>>;

export class MspError extends Error {
  readonly code: string;
  readonly [key: string]: unknown;
}

export class MspArgumentError extends MspError {
  readonly code: "ERR_MSP_ARGUMENT";
  readonly parameter: string;
}

export class MspNativeError extends MspError {
  readonly code: string;
  readonly path?: string;
  readonly operation?: string;
}

export class MspArchitectureError extends MspNativeError {
  readonly code: "ERR_MSP_ARCHITECTURE";
  readonly architecture?: string;
  readonly machine?: number;
  readonly expectedMachine?: number;
}

export class MspPlatformError extends MspNativeError {
  readonly code: "ERR_MSP_PLATFORM";
  readonly platform?: string;
}

export class MspDisposedError extends MspError {
  readonly code: "ERR_MSP_DISPOSED";
  readonly owner: string;
}

export class MspResultError extends MspError {
  readonly code: "ERR_MSP_RESULT";
  readonly operation: string;
  readonly exitCode: number;
  readonly stderr: Uint8Array;
}

export class MspRuntime {
  private constructor();
  readonly dllPath: string;
  readonly architecture: string;
  readonly machine: number;
  readonly abiVersion: number;
  readonly version: string;
  readonly info: RuntimeInfo;
  createWorkspace(): MspWorkspace;
  createDefaultSession(): MspSession;
  createSession(workspace?: MspWorkspace): MspSession;
  close(): void;
  dispose(): void;
}

export class MspWorkspace {
  private constructor();
  readonly closed: boolean;
  createSession(): MspSession;
  putFile(virtualPath: string, data: ByteSource): number;
  addFile(virtualPath: string, data: ByteSource): number;
  createDirectory(virtualPath: string): number;
  stat(virtualPath: string): MspResult;
  list(virtualPath: string): MspResult;
  listDirectory(virtualPath: string): MspResult;
  read(virtualPath: string, offset: NativeOffset, length: number): MspResult;
  readFileRange(virtualPath: string, offset: NativeOffset, length: number): MspResult;
  close(): void;
  dispose(): void;
}

export class MspSession {
  private constructor();
  readonly closed: boolean;
  run(command: string | ByteSource): MspResult;
  runBytes(command: ByteSource): MspResult;
  close(): void;
  dispose(): void;
}

/**
 * A worker-safe result snapshot. Native bytes are copied and the native result
 * handle is freed before this object is returned to the caller.
 */
export class MspResult {
  private constructor();
  readonly exitCode: number;
  readonly status: number;
  readonly exit: number;
  readonly ok: boolean;
  readonly isSuccess: boolean;
  readonly stdout: Uint8Array;
  readonly stderr: Uint8Array;
  readonly stdoutBytes: Uint8Array;
  readonly stderrBytes: Uint8Array;
  copyStdout(): Uint8Array;
  copyStderr(): Uint8Array;
  ensureSuccess(operation?: string): void;
  toWorkerData(): { exitCode: number; stdout: Uint8Array; stderr: Uint8Array };
  close(): void;
  dispose(): void;
}

export function validateVirtualPath(value: string, parameter?: string): Uint8Array;
export function normalizeAbsoluteDllPath(dllPath: string): string;
export function inspectPeMachine(dllPath: string, fs?: DllInspectionFs): number;
export function selectDll(dllPath: string, options?: LoadOptions): DllSelection;
export function createKoffiBackend(koffi: unknown, dllPath: string): NativeBackend;
export function loadMspFfi(dllPath: string, options?: LoadOptions): MspRuntime;
export const loadNativeLibrary: typeof loadMspFfi;
