"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");

const packageRoot = path.resolve(__dirname, "..");
const packageJson = JSON.parse(fs.readFileSync(path.join(packageRoot, "package.json"), "utf8"));
const source = fs.readFileSync(path.join(packageRoot, "index.js"), "utf8");
const {
  ABI_VERSION,
  LIMITS,
  MACHINE_FOR_ARCH,
  MspArchitectureError,
  MspArgumentError,
  MspPlatformError,
  RUNTIME_VERSION,
  inspectPeMachine,
  loadMspFfi,
  normalizeAbsoluteDllPath,
  selectDll,
  validateVirtualPath,
} = require(packageRoot);
const { makeFakeBackend, makePeFixture } = require("./support");

test("package metadata has no native install lifecycle and exposes declarations", () => {
  for (const scriptName of ["install", "preinstall", "postinstall"]) {
    assert.equal(packageJson.scripts?.[scriptName], undefined, `${scriptName} must not exist`);
  }
  assert.equal(packageJson.types, "index.d.ts");
  assert.equal(packageJson.main, "index.js");
  assert.equal(packageJson.peerDependenciesMeta.koffi.optional, true);
  assert.equal(fs.existsSync(path.join(packageRoot, "index.d.ts")), true);
  assert.doesNotMatch(source, /node:child_process|process\.env\.PATH|\.spawn\s*\(|\.exec\s*\(/);
});

test("DLL selection requires an absolute .dll and validates PE architecture before loading", () => {
  assert.throws(() => normalizeAbsoluteDllPath("msp_ffi.dll"), MspArgumentError);
  assert.throws(() => normalizeAbsoluteDllPath("C:\\native\\msp_ffi.so"), MspArgumentError);
  assert.throws(() => selectDll("C:\\native\\msp_ffi.dll", { platform: "linux" }), MspPlatformError);

  const architecture = process.arch;
  const machine = MACHINE_FOR_ARCH[architecture];
  assert.notEqual(machine, undefined, `test host architecture ${architecture} is unsupported`);
  const fixture = makePeFixture(machine);
  try {
    assert.equal(inspectPeMachine(fixture.dllPath), machine);
    const selection = selectDll(fixture.dllPath, { platform: "win32", arch: architecture });
    assert.equal(selection.path, path.normalize(fixture.dllPath));
    assert.equal(selection.machine, machine);

    const wrongMachine = machine === MACHINE_FOR_ARCH.x64
      ? MACHINE_FOR_ARCH.ia32
      : MACHINE_FOR_ARCH.x64;
    const wrong = makePeFixture(wrongMachine);
    try {
      assert.throws(
        () => selectDll(wrong.dllPath, { platform: "win32", arch: architecture }),
        (error) => error instanceof MspArchitectureError
          && error.expectedMachine === machine
          && error.machine === wrongMachine,
      );
    } finally {
      wrong.cleanup();
    }
  } finally {
    fixture.cleanup();
  }
});

test("virtual path validation rejects host and traversal syntax", () => {
  assert.deepEqual(validateVirtualPath("/docs/note.txt"), Uint8Array.from(Buffer.from("/docs/note.txt")));
  for (const value of [
    "relative.txt",
    "C:\\secret",
    "//server/share",
    "/docs/file:ads",
    "/docs/../secret",
    "/.msp/state",
    "/CON",
    "/docs/bad?.txt",
  ]) {
    assert.throws(() => validateVirtualPath(value), MspArgumentError, value);
  }
  assert.throws(() => validateVirtualPath("/" + "x".repeat(LIMITS.MAX_PATH_BYTES)), MspArgumentError);
});

test("ABI constants are pinned to the public header contract", () => {
  assert.equal(ABI_VERSION, 1);
  assert.equal(RUNTIME_VERSION, "0.1.0");
  const fixture = makePeFixture(MACHINE_FOR_ARCH[process.arch]);
  try {
    const runtime = loadMspFfi(fixture.dllPath, {
      platform: "win32",
      arch: process.arch,
      backend: makeFakeBackend(),
    });
    assert.equal(runtime.abiVersion, ABI_VERSION);
    assert.equal(runtime.version, RUNTIME_VERSION);
    runtime.close();
  } finally {
    fixture.cleanup();
  }
});
