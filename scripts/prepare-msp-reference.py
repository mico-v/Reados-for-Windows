#!/usr/bin/env python3
"""Prepare an ignored MSP reference tree from a verified source archive."""

from __future__ import annotations

import argparse
import hashlib
import shutil
import tempfile
import zipfile
from pathlib import Path, PurePosixPath


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def safe_member_path(name: str) -> PurePosixPath:
    path = PurePosixPath(name)
    if path.is_absolute() or any(part in {"", ".", ".."} for part in path.parts):
        raise ValueError(f"unsafe archive member path: {name}")
    if len(path.parts) > 0 and ":" in path.parts[0]:
        raise ValueError(f"unsafe archive member path: {name}")
    return path


def extract_archive(archive: Path, destination: Path) -> Path:
    with zipfile.ZipFile(archive) as source:
        members = [(info, safe_member_path(info.filename)) for info in source.infolist()]
        roots = {
            path.parts[0]
            for _, path in members
            if path.parts and path.parts[0] not in {".", ".."}
        }
        if len(roots) != 1:
            raise ValueError(f"archive must have exactly one top-level directory: {sorted(roots)}")
        root = next(iter(roots))
        required = PurePosixPath(root) / "Implementations" / "Windows" / "Cargo.toml"
        if not any(path == required for _, path in members):
            raise ValueError("archive does not contain Implementations/Windows/Cargo.toml")

        with tempfile.TemporaryDirectory(prefix="msp-reference-") as temporary:
            staging = Path(temporary)
            for info, relative in members:
                if info.is_dir():
                    continue
                target = staging.joinpath(*relative.parts)
                target.parent.mkdir(parents=True, exist_ok=True)
                with source.open(info) as input_stream, target.open("wb") as output_stream:
                    shutil.copyfileobj(input_stream, output_stream)

            extracted_root = staging / root
            destination.mkdir(parents=True, exist_ok=True)
            for item in extracted_root.iterdir():
                target = destination / item.name
                if item.is_dir():
                    shutil.copytree(item, target, dirs_exist_ok=True)
                else:
                    shutil.copy2(item, target)

    return destination / "Implementations" / "Windows" / "Cargo.toml"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--archive", required=True, type=Path)
    parser.add_argument("--destination", default=Path("MSP"), type=Path)
    parser.add_argument("--sha256", required=True)
    args = parser.parse_args()

    archive = args.archive.resolve()
    if not archive.is_file():
        raise SystemExit(f"archive not found: {archive}")
    expected = args.sha256.lower().strip()
    if len(expected) != 64 or any(char not in "0123456789abcdef" for char in expected):
        raise SystemExit("--sha256 must be a 64-character hexadecimal SHA-256 digest")
    actual = sha256(archive)
    if actual != expected:
        raise SystemExit(f"archive SHA-256 mismatch: expected {expected}, got {actual}")

    cargo_manifest = extract_archive(archive, args.destination.resolve())
    print(f"Prepared MSP reference: {cargo_manifest}")
    print(f"Archive SHA-256: {actual}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
