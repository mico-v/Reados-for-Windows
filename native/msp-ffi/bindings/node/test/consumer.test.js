"use strict";

const assert = require("node:assert/strict");
const test = require("node:test");

const {
  LIMITS,
  MACHINE_FOR_ARCH,
  MspArgumentError,
  MspDisposedError,
  MspResultError,
  STATUS,
  loadMspFfi,
} = require("..");
const { makeFakeBackend, makePeFixture, workerRoundTrip } = require("./support");

test("offline consumer preserves virtual bytes, ownership, and worker-safe results", async () => {
  const fixture = makePeFixture(MACHINE_FOR_ARCH[process.arch]);
  const backend = makeFakeBackend();
  try {
    const runtime = loadMspFfi(fixture.dllPath, {
      platform: "win32",
      arch: process.arch,
      backend,
    });
    assert.equal(runtime.info.abiVersion, 1);
    assert.equal(runtime.info.version, "0.1.0");

    const workspace = runtime.createWorkspace();
    const binary = Uint8Array.from([0x00, 0xff, 0x41, 0x0a]);
    assert.equal(workspace.putFile("/bytes.bin", binary), STATUS.OK);
    assert.equal(workspace.createDirectory("/docs"), STATUS.OK);
    assert.equal(workspace.addFile("/docs/name.bin", binary), STATUS.OK);

    const read = workspace.readFileRange("/bytes.bin", 0, binary.length);
    assert.equal(read.exitCode, STATUS.OK);
    assert.deepEqual(read.stdout, binary);
    assert.deepEqual(read.stderr, new Uint8Array());
    const mutableCopy = read.stdout;
    mutableCopy[0] = 0x7f;
    assert.deepEqual(read.stdout, binary, "result accessors must return copies");
    assert.equal(read.close(), undefined);
    assert.equal(read.close(), undefined);

    const listing = workspace.listDirectory("/docs");
    assert.equal(listing.exitCode, STATUS.OK);
    assert.match(new TextDecoder().decode(listing.stdout), /name\.bin/);

    const session = workspace.createSession();
    const command = session.run("echo node-consumer");
    assert.equal(command.exitCode, STATUS.OK);
    assert.deepEqual(command.stdout, Uint8Array.from(Buffer.from("node-consumer\n")));
    assert.equal(backend.freedResults.size, 3);

    const workerData = await workerRoundTrip(command);
    assert.deepEqual(workerData, {
      exitCode: STATUS.OK,
      stdout: [...Buffer.from("node-consumer\n")],
      stderr: [],
    });
    assert.deepEqual(command.toWorkerData(), {
      exitCode: STATUS.OK,
      stdout: Uint8Array.from(Buffer.from("node-consumer\n")),
      stderr: new Uint8Array(),
    });

    const binaryResult = session.run("bytes");
    assert.deepEqual(binaryResult.stdoutBytes, binary);
    assert.equal(binaryResult.ok, true);

    const failed = session.run("not-registered");
    assert.equal(failed.exitCode, STATUS.INVALID_ARGUMENT);
    assert.throws(
      () => failed.ensureSuccess("registered command"),
      (error) => error instanceof MspResultError
        && error.exitCode === STATUS.INVALID_ARGUMENT
        && error.stderr[0] === "u".charCodeAt(0),
    );

    // Native session creation retains its workspace reference independently.
    workspace.close();
    workspace.close();
    assert.equal(workspace.closed, true);
    const retained = session.run("echo retained");
    assert.deepEqual(retained.stdout, Uint8Array.from(Buffer.from("retained\n")));
    session.close();
    session.close();
    assert.equal(session.closed, true);
    assert.throws(() => session.run("echo after-close"), MspDisposedError);

    // There is no process-launch surface in the public contract.
    assert.equal(typeof session.spawn, "undefined");
    assert.equal(typeof session.exec, "undefined");
    assert.equal(typeof workspace.hostRoot, "undefined");
    runtime.close();
  } finally {
    fixture.cleanup();
  }
});

test("offline consumer validates command bytes before crossing the ABI", () => {
  const fixture = makePeFixture(MACHINE_FOR_ARCH[process.arch]);
  try {
    const runtime = loadMspFfi(fixture.dllPath, {
      platform: "win32",
      arch: process.arch,
      backend: makeFakeBackend(),
    });
    const session = runtime.createDefaultSession();
    for (const command of ["echo\0hidden", "\ud800"]) {
      assert.throws(() => session.run(command), MspArgumentError);
    }
    assert.throws(() => session.run(Uint8Array.from([0xff])), MspArgumentError);
    assert.throws(
      () => session.run(new Uint8Array(LIMITS.MAX_COMMAND_BYTES + 1)),
      MspArgumentError,
    );
    session.close();
  } finally {
    fixture.cleanup();
  }
});

test("offline consumer rejects host paths before invoking the fake ABI", () => {
  const fixture = makePeFixture(MACHINE_FOR_ARCH[process.arch]);
  const backend = makeFakeBackend();
  let mutationCalls = 0;
  const original = backend.workspacePutFile;
  backend.workspacePutFile = (...args) => {
    mutationCalls += 1;
    return original(...args);
  };
  try {
    const runtime = loadMspFfi(fixture.dllPath, {
      platform: "win32",
      arch: process.arch,
      backend,
    });
    const workspace = runtime.createWorkspace();
    for (const virtualPath of ["C:\\secret", "//server/share", "/docs/../secret", "/.msp/audit"]) {
      assert.throws(() => workspace.putFile(virtualPath, Uint8Array.of(1)), MspArgumentError);
    }
    assert.equal(mutationCalls, 0);
    workspace.close();
  } finally {
    fixture.cleanup();
  }
});
