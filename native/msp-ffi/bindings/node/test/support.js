"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { TextDecoder } = require("node:util");
const { Worker } = require("node:worker_threads");

const api = require("..");

function makePeFixture(machine) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "msp-ffi-node-"));
  const dllPath = path.join(directory, "msp_ffi.dll");
  const image = Buffer.alloc(0x100, 0);
  image.write("MZ", 0, "ascii");
  image.writeUInt32LE(0x80, 0x3c);
  image.write("PE\0\0", 0x80, "ascii");
  image.writeUInt16LE(machine, 0x84);
  fs.writeFileSync(dllPath, image);
  return {
    dllPath,
    cleanup() {
      fs.rmSync(directory, { recursive: true, force: true });
    },
  };
}

function pathFromCString(value) {
  const bytes = Buffer.from(value);
  const nul = bytes.indexOf(0);
  return bytes.subarray(0, nul < 0 ? bytes.length : nul).toString("utf8");
}

function bytesOf(value) {
  return value == null ? new Uint8Array() : Uint8Array.from(value);
}

function makeFakeBackend() {
  let nextResult = 1;
  const freedResults = new Set();
  const resultSnapshots = new Map();

  function result(exitCode, stdout = [], stderr = []) {
    const handle = { kind: "result", id: nextResult++ };
    resultSnapshots.set(handle, {
      exitCode,
      stdout: bytesOf(stdout),
      stderr: bytesOf(stderr),
    });
    return handle;
  }

  function decodeCommand(command, length) {
    return new TextDecoder("utf-8", { fatal: true }).decode(
      command == null ? new Uint8Array() : bytesOf(command).subarray(0, length),
    );
  }

  const backend = {
    freedResults,
    runtimeAbiVersion: () => 1,
    runtimeVersion: () => "0.1.0",
    workspaceCreate: () => ({ kind: "workspace", files: new Map(), closed: false }),
    workspaceFree: (workspace) => { workspace.closed = true; },
    workspacePutFile: (workspace, virtualPath, data, length) => {
      workspace.files.set(pathFromCString(virtualPath), bytesOf(data).subarray(0, length));
      return 0;
    },
    workspaceAddFile: (workspace, virtualPath, data, length) => {
      workspace.files.set(pathFromCString(virtualPath), bytesOf(data).subarray(0, length));
      return 0;
    },
    workspaceCreateDirectory: (workspace, virtualPath) => {
      workspace.files.set(pathFromCString(virtualPath), null);
      return 0;
    },
    sessionCreate: (workspace) => ({ kind: "session", workspace, closed: false }),
    sessionCreateDefault: () => ({ kind: "session", workspace: null, closed: false }),
    sessionFree: (session) => { session.closed = true; },
    sessionRunN: (session, command, length) => {
      const text = decodeCommand(command, length);
      if (text.startsWith("echo ")) {
        return result(0, Buffer.from(`${text.slice(5)}\n`, "utf8"));
      }
      if (text === "bytes") {
        return result(0, [0x00, 0xff, 0x41, 0x0a]);
      }
      return result(2, [], Buffer.from("unknown command\n"));
    },
    resultSnapshot: (handle) => {
      const snapshot = resultSnapshots.get(handle);
      if (!snapshot) throw new Error("unknown result");
      return {
        exitCode: snapshot.exitCode,
        stdout: Uint8Array.from(snapshot.stdout),
        stderr: Uint8Array.from(snapshot.stderr),
      };
    },
    resultFree: (handle) => {
      freedResults.add(handle.id);
      resultSnapshots.delete(handle);
    },
    workspaceStat: (workspace, virtualPath) => {
      const key = pathFromCString(virtualPath);
      const data = workspace.files.get(key);
      if (!workspace.files.has(key)) return result(1, [], Buffer.from("not found\n"));
      return result(0, Buffer.from(JSON.stringify({
        ok: true,
        fileInfo: { fileType: data === null ? "directory" : "regularFile", sizeBytes: data?.length ?? null },
      })));
    },
    workspaceList: (workspace, virtualPath) => {
      const prefix = pathFromCString(virtualPath) === "/" ? "/" : `${pathFromCString(virtualPath)}/`;
      const entries = [...workspace.files.keys()]
        .filter((key) => key.startsWith(prefix) && !key.slice(prefix.length).includes("/"))
        .map((key) => ({ name: key.slice(prefix.length), info: {} }));
      return result(0, Buffer.from(JSON.stringify({ ok: true, entries })));
    },
    workspaceListDirectory: function workspaceListDirectory(workspace, virtualPath) {
      return this.workspaceList(workspace, virtualPath);
    },
    workspaceRead: (workspace, virtualPath, offset, length) => {
      const data = workspace.files.get(pathFromCString(virtualPath));
      if (!data) return result(1, [], Buffer.from("not found\n"));
      const start = Number(offset);
      return result(0, data.slice(start, start + length));
    },
    workspaceReadFileRange: function workspaceReadFileRange(workspace, virtualPath, offset, length) {
      return this.workspaceRead(workspace, virtualPath, offset, length);
    },
  };
  return backend;
}

function workerRoundTrip(value) {
  return new Promise((resolve, reject) => {
    const worker = new Worker(
      "const { parentPort, workerData } = require('node:worker_threads');"
        + " parentPort.postMessage({ exitCode: workerData.exitCode, stdout: Array.from(workerData.stdout), stderr: Array.from(workerData.stderr) });",
      { eval: true, workerData: value },
    );
    worker.once("message", resolve);
    worker.once("error", reject);
  });
}

module.exports = {
  api,
  makePeFixture,
  makeFakeBackend,
  workerRoundTrip,
};
