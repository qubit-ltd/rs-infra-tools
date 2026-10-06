#!/usr/bin/env python3
"""Install the centrally maintained bootstrap files into one project."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import stat
import subprocess
import sys
import tempfile


SNAPSHOT = Path(".infra/bootstrap-source.json")
TEMP_MARKER = ".rs-infra-bootstrap-temp"


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def executable_mode_matches(actual: bool, expected: bool, *, windows: bool | None = None) -> bool:
    if windows is None:
        windows = os.name == "nt"
    return windows or actual == expected


def safe_path(root: Path, value: str) -> Path:
    rel = PurePosixPath(value)
    if rel.is_absolute() or not rel.parts or ".." in rel.parts or "\\" in value:
        raise ValueError(f"unsafe repository path: {value!r}")
    path = root.joinpath(*rel.parts)
    resolved_root = root.resolve()
    resolved_parent = path.parent.resolve()
    if resolved_parent != resolved_root and resolved_root not in resolved_parent.parents:
        raise ValueError(f"path escapes project root: {value!r}")
    if path.is_symlink():
        raise ValueError(f"refusing symlink target: {value!r}")
    return path


def load_manifest(package: Path) -> dict:
    manifest_path = safe_path(package, "manifest.json")
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if manifest.get("version") != 1 or not isinstance(manifest.get("files"), list) or not isinstance(manifest.get("profiles", {}), dict):
        raise ValueError("unsupported or invalid project-bootstrap manifest")
    targets: set[str] = set()
    all_entries = list(manifest["files"])
    for profile, entries in manifest.get("profiles", {}).items():
        if not profile or not isinstance(entries, list):
            raise ValueError(f"invalid profile {profile!r}")
        all_entries.extend(entries)
    for entry in all_entries:
        source, target = entry.get("source"), entry.get("target")
        if not isinstance(source, str) or not isinstance(target, str):
            raise ValueError("manifest source and target must be strings")
        safe_path(package, source)
        safe_path(Path("/project-root"), target)
        if target in targets:
            raise ValueError(f"duplicate manifest target: {target}")
        targets.add(target)
        if not isinstance(entry.get("executable"), bool):
            raise ValueError(f"missing executable flag for {target}")
    return manifest


def selected_entries(manifest: dict, project: Path) -> list[dict]:
    entries = list(manifest["files"])
    config = safe_path(project, ".infra/bootstrap-update.toml")
    if not config.exists():
        return entries
    if config.is_symlink() or not config.is_file():
        raise ValueError(".infra/bootstrap-update.toml must be a regular file")
    profiles = []
    for line in config.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        key, separator, value = line.partition("=")
        if not separator or key.strip() != "profiles":
            raise ValueError("bootstrap-update.toml only supports the profiles array")
        profiles = json.loads(value.strip())
    if not isinstance(profiles, list) or any(not isinstance(name, str) for name in profiles):
        raise ValueError("profiles must be an array of strings in bootstrap-update.toml")
    known = manifest.get("profiles", {})
    for name in profiles:
        if name not in known:
            raise ValueError(f"unknown bootstrap profile: {name}")
        entries.extend(known[name])
    return entries


def package_state(package: Path, manifest: dict) -> str:
    hasher = hashlib.sha256()
    manifest_bytes = (package / "manifest.json").read_bytes()
    hasher.update(b"manifest.json\0" + manifest_bytes)
    entries = list(manifest["files"])
    for profile_entries in manifest.get("profiles", {}).values():
        entries.extend(profile_entries)
    for entry in entries:
        source = safe_path(package, entry["source"])
        data = source.read_bytes()
        executable = entry["executable"]
        hasher.update(entry["target"].encode() + b"\0" + data + (b"\1" if executable else b"\0"))
    return hasher.hexdigest()


def project_state(project: Path) -> tuple[dict, list[str]]:
    snap_path = safe_path(project, SNAPSHOT.as_posix())
    if not snap_path.is_file():
        return {}, [SNAPSHOT.as_posix()]
    snapshot = json.loads(snap_path.read_text(encoding="utf-8"))
    drift: list[str] = []
    recorded = snapshot.get("files", {})
    for target, expected in recorded.items():
        path = safe_path(project, target)
        if not path.is_file():
            drift.append(target)
            continue
        mode = bool(path.stat().st_mode & stat.S_IXUSR)
        if digest(path.read_bytes()) != expected.get("sha256") or not executable_mode_matches(mode, expected.get("executable")):
            drift.append(target)
    return snapshot, drift


def revision(package: Path) -> str:
    value = os.environ.get("RS_INFRA_SOURCE_REVISION")
    if value:
        return value
    try:
        return subprocess.check_output(["git", "-C", str(package), "rev-parse", "HEAD"], text=True, stderr=subprocess.DEVNULL).strip()
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def cleanup_downloaded_package() -> None:
    value = os.environ.pop("RS_INFRA_BOOTSTRAP_TEMP_ROOT", None)
    if not value:
        return
    root = Path(value)
    marker = root / TEMP_MARKER
    try:
        if root.is_symlink() or not root.is_dir() or not root.name.startswith("rs-infra-bootstrap."):
            return
        if marker.read_text(encoding="utf-8") != "rs-infra-bootstrap-temp-v1\n":
            return
        shutil.rmtree(root)
    except OSError as error:
        print(f"warning: unable to clean bootstrap download {root}: {error}", file=sys.stderr)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project-root", type=Path, required=True)
    parser.add_argument("--package-root", type=Path, default=Path(__file__).resolve().parent)
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument("--yes", action="store_true", help="overwrite managed files without prompting")
    modes.add_argument("--dry-run", action="store_true", help="list targets without writing")
    modes.add_argument("--status", action="store_true", help="compare this package with the installed snapshot")
    args = parser.parse_args()
    project = args.project_root.resolve()
    package = args.package_root.resolve()
    try:
        try:
            manifest = load_manifest(package)
            entries = selected_entries(manifest, project)
            package_digest = package_state(package, manifest)
            package_files = {entry["target"]: {"sha256": digest(safe_path(package, entry["source"]).read_bytes()), "executable": entry["executable"]} for entry in entries}
            old_snapshot, drift = project_state(project)
            if args.status:
                current = old_snapshot.get("package_sha256")
                if current == package_digest:
                    print("bootstrap snapshot matches upstream package")
                    return 0
                print(f"bootstrap package differs: installed={current or 'missing'} upstream={package_digest}")
                if drift:
                    print("locally modified or missing: " + ", ".join(drift))
                return 1

            targets = [entry["target"] for entry in entries] + [SNAPSHOT.as_posix()]
            print(f"rs-infra-tools {revision(package)} will overwrite {len(targets)} paths:")
            for target in targets:
                print(f"  {target}")
            if args.dry_run:
                return 0
            if not args.yes:
                if not sys.stdin.isatty():
                    print("error: confirmation requires an interactive terminal; pass --yes to overwrite", file=sys.stderr)
                    return 2
                answer = input("Overwrite these paths with the upstream versions? [y/N] ").strip().lower()
                if answer not in {"y", "yes"}:
                    print("cancelled")
                    return 1

            planned: list[tuple[dict, Path, bytes]] = []
            for entry in entries:
                source = safe_path(package, entry["source"])
                target = safe_path(project, entry["target"])
                if target.exists() and not target.is_file():
                    raise ValueError(f"target is not a regular file: {entry['target']}")
                planned.append((entry, target, source.read_bytes()))
            for entry, target, data in planned:
                target.parent.mkdir(parents=True, exist_ok=True)
                fd, temp_name = tempfile.mkstemp(prefix=".infra-update-", dir=target.parent)
                try:
                    with os.fdopen(fd, "wb") as stream:
                        stream.write(data)
                    target_mode = 0o755 if entry["executable"] else 0o644
                    os.chmod(temp_name, target_mode)
                    os.replace(temp_name, target)
                finally:
                    if os.path.exists(temp_name):
                        os.unlink(temp_name)
            source_revision = revision(package)
            snapshot = {"schema": 1, "source_repository": "https://github.com/qubit-ltd/rs-infra-tools", "source_revision": source_revision, "package_sha256": package_digest, "files": package_files}
            snap_path = safe_path(project, SNAPSHOT.as_posix())
            snap_path.parent.mkdir(parents=True, exist_ok=True)
            snap_path.write_text(json.dumps(snapshot, indent=2, sort_keys=True) + "\n", encoding="utf-8")
            os.chmod(snap_path, 0o644)
            print("bootstrap files updated")
            return 0
        except (OSError, ValueError, json.JSONDecodeError) as error:
            print(f"error: {error}", file=sys.stderr)
            return 2
    finally:
        cleanup_downloaded_package()


if __name__ == "__main__":
    raise SystemExit(main())
