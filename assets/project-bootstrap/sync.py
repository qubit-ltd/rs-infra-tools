#!/usr/bin/env python3
"""Install the centrally maintained bootstrap files into one project."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import time


SNAPSHOT = Path(".infra/bootstrap-source.json")
TEMP_MARKER = ".rs-infra-bootstrap-temp"


def digest(data: bytes) -> str:
    # Git for Windows may materialize managed text files with CRLF endings.
    # Hash their canonical LF form so snapshots stay portable across checkouts.
    return hashlib.sha256(data.replace(b"\r\n", b"\n")).hexdigest()


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
    sources = manifest.get("config_sources", [])
    if not isinstance(sources, list):
        raise ValueError("config_sources must be an array")
    names: set[str] = set()
    directories: set[str] = set()
    for source in sources:
        if not isinstance(source, dict):
            raise ValueError("invalid config source")
        name, directory, repository = (source.get(key) for key in ("name", "directory", "repository"))
        local = source.get("local")
        if not all(isinstance(value, str) and value for value in (name, directory)):
            raise ValueError("config source name and directory must be nonempty strings")
        if (isinstance(repository, str) and bool(repository)) == (local == "manager"):
            raise ValueError(f"config source {name} must specify repository or local manager")
        if local is not None and local != "manager":
            raise ValueError(f"invalid local config source: {local!r}")
        if name in names or directory in directories:
            raise ValueError(f"duplicate config source: {name}")
        if "/" in name or "\\" in name or name in {".", ".."}:
            raise ValueError(f"invalid config source name: {name}")
        if "/" in directory or "\\" in directory or directory in {".", ".."}:
            raise ValueError(f"invalid config directory: {directory}")
        names.add(name)
        directories.add(directory)
    retired = manifest.get("retired_targets", [])
    if not isinstance(retired, list) or any(not isinstance(target, str) for target in retired):
        raise ValueError("retired_targets must be an array of paths")
    if len(retired) != len(set(retired)):
        raise ValueError("duplicate retired target")
    for target in retired:
        safe_path(Path("/project-root"), target)
        if target == SNAPSHOT.as_posix():
            raise ValueError("cannot retire bootstrap snapshot")
    return manifest


def config_entries(manifest: dict, checkout_root: Path, package_root: Path | None = None) -> tuple[list[tuple[dict, Path]], dict[str, str]]:
    """Resolve each owner's declared conf files, fetching external sources."""
    entries: list[tuple[dict, Path]] = []
    versions: dict[str, str] = {}
    targets = {entry["target"] for entry in manifest["files"]}
    for profile_entries in manifest.get("profiles", {}).values():
        targets.update(entry["target"] for entry in profile_entries)
    for source in manifest.get("config_sources", []):
        if source.get("local") == "manager":
            if package_root is None:
                raise ValueError("manager config source requires package root")
            checkout = package_root.parent.parent
        else:
            for attempt in range(1, 5):
                checkout = checkout_root / f"{source['name']}-{attempt}"
                try:
                    subprocess.run(
                        ["git", "clone", "--quiet", "--depth", "1", "--single-branch", "--branch", "main", source["repository"], str(checkout)],
                        check=True,
                    )
                    break
                except subprocess.CalledProcessError:
                    if attempt == 4:
                        raise
                    delay = 2**attempt
                    print(f"warning: clone {source['name']} failed (attempt {attempt}/4); retrying in {delay}s", file=sys.stderr)
                    time.sleep(delay)
        conf = checkout / "conf"
        config_manifest_path = safe_path(conf, "manifest.json")
        if not config_manifest_path.is_file() or config_manifest_path.is_symlink():
            raise ValueError(f"missing regular conf/manifest.json in {source['name']}")
        manifest_bytes = config_manifest_path.read_bytes().replace(b"\r\n", b"\n")
        config_manifest = json.loads(manifest_bytes)
        files = config_manifest.get("files")
        if config_manifest.get("version") != 1 or not isinstance(files, list):
            raise ValueError(f"invalid conf/manifest.json in {source['name']}")
        hasher = hashlib.sha256()
        hasher.update(b"manifest.json\0" + manifest_bytes)
        for item in files:
            if not isinstance(item, dict) or not isinstance(item.get("source"), str) or not isinstance(item.get("target"), str):
                raise ValueError(f"invalid config entry in {source['name']}")
            path = safe_path(conf, item["source"])
            if not path.is_file() or path.is_symlink():
                raise ValueError(f"missing regular config file {item['source']} in {source['name']}")
            hasher.update(item["source"].encode() + b"\0" + path.read_bytes().replace(b"\r\n", b"\n"))
            relative_target = PurePosixPath(item["target"])
            if relative_target.is_absolute() or not relative_target.parts or ".." in relative_target.parts or "\\" in item["target"]:
                raise ValueError(f"unsafe config target: {item['target']!r}")
            target = f".infra/{source['directory']}/{item['target']}"
            safe_path(Path("/project-root"), target)
            if target in targets:
                raise ValueError(f"duplicate manifest target: {target}")
            targets.add(target)
            entries.append(({"target": target, "executable": False}, path))
        if source["name"] == "rs-infra-dependency":
            current = conf / "policy/current.toml"
            if not current.is_file() or current.is_symlink():
                raise ValueError("dependency current baseline pointer is missing")
            if not {".infra/dependency/policy.toml", ".infra/dependency/policy/current.toml"} <= targets:
                raise ValueError("dependency policy and current pointer must be installed together")
            match = re.fullmatch(r'\s*baseline\s*=\s*"([A-Za-z0-9._-]+)"\s*', current.read_text(encoding="utf-8"))
            baseline = match.group(1) if match else None
            expected = {f".infra/dependency/policy/baselines/{baseline}.{suffix}" for suffix in ("txt", "toml")}
            if baseline is None or len(expected & targets) != 1:
                raise ValueError("dependency current baseline is absent from conf/manifest.json")
        versions[source["name"]] = "sha256:" + hasher.hexdigest()
    return entries, versions


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
            if name == "dependency-policy-default":
                raise ValueError(
                    "obsolete bootstrap profile dependency-policy-default; remove it from "
                    ".infra/bootstrap-update.toml (dependency policy is installed automatically)"
                )
            raise ValueError(f"unknown bootstrap profile: {name}")
        entries.extend(known[name])
    return entries


def package_state(package: Path, manifest: dict) -> str:
    hasher = hashlib.sha256()
    manifest_bytes = (package / "manifest.json").read_bytes()
    hasher.update(b"manifest.json\0" + manifest_bytes.replace(b"\r\n", b"\n"))
    entries = list(manifest["files"])
    for profile_entries in manifest.get("profiles", {}).values():
        entries.extend(profile_entries)
    for entry in entries:
        source = safe_path(package, entry["source"])
        data = source.read_bytes()
        executable = entry["executable"]
        canonical_data = data.replace(b"\r\n", b"\n")
        hasher.update(entry["target"].encode() + b"\0" + canonical_data + (b"\1" if executable else b"\0"))
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


def obsolete_baselines(project: Path, snapshot: dict, current_targets: set[str]) -> list[Path]:
    """Find previous managed baselines that the current manifest no longer installs."""
    obsolete = []
    for target, record in snapshot.get("files", {}).items():
        if not target.startswith(".infra/dependency/policy/baselines/") or target in current_targets:
            continue
        path = safe_path(project, target)
        if not path.is_file() or path.is_symlink() or digest(path.read_bytes()) != record.get("sha256"):
            raise ValueError(f"obsolete baseline was modified or is missing: {target}")
        obsolete.append(path)
    return obsolete


def retired_config_paths(project: Path, manifest: dict, snapshot: dict, current_targets: set[str]) -> list[Path]:
    """Retire only targets still matching their previous managed snapshot."""
    obsolete = []
    recorded = snapshot.get("files", {})
    for target in manifest.get("retired_targets", []):
        if target in current_targets:
            continue
        path = safe_path(project, target)
        record = recorded.get(target)
        if record is None:
            if path.exists():
                raise ValueError(f"retired config is not recorded in snapshot: {target}")
            continue
        if not path.is_file() or digest(path.read_bytes()) != record.get("sha256"):
            raise ValueError(f"retired config was modified or is missing: {target}")
        mode = bool(path.stat().st_mode & stat.S_IXUSR)
        if not executable_mode_matches(mode, record.get("executable")):
            raise ValueError(f"retired config mode was modified: {target}")
        obsolete.append(path)
    return obsolete


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
    parser.add_argument("--configs-only", action="store_true", help="install shared config without replacing project bootstrap scripts")
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument("--yes", action="store_true", help="overwrite managed files without prompting")
    modes.add_argument("--dry-run", action="store_true", help="list targets without writing")
    modes.add_argument("--status", action="store_true", help="compare this package with the installed snapshot")
    modes.add_argument("--check", action="store_true", help="check the installed snapshot without fetching")
    args = parser.parse_args()
    project = args.project_root.resolve()
    package = args.package_root.resolve()
    try:
        try:
            if args.check:
                _, drift = project_state(project)
                if drift:
                    print("locally modified or missing: " + ", ".join(drift))
                    return 1
                print("installed shared config snapshot matches")
                return 0
            manifest = load_manifest(package)
            entries = [] if args.configs_only else selected_entries(manifest, project)
            package_digest = package_state(package, manifest)
            with tempfile.TemporaryDirectory(prefix="rs-infra-config-") as checkout_directory:
                external, config_versions = config_entries(manifest, Path(checkout_directory), package)
                planned_sources = [(entry, safe_path(package, entry["source"])) for entry in entries] + external
                package_files = {entry["target"]: {"sha256": digest(source.read_bytes()), "executable": entry["executable"]} for entry, source in planned_sources}
                old_snapshot, drift = project_state(project)
                if args.status:
                    current = old_snapshot.get("package_sha256")
                    if current == package_digest and old_snapshot.get("config_sources", {}) == config_versions and not drift:
                        print("bootstrap snapshot matches upstream package and shared config")
                        return 0
                    print(f"bootstrap package differs: installed={current or 'missing'} upstream={package_digest}")
                    if old_snapshot.get("config_sources", {}) != config_versions:
                        print("shared config contents differ")
                    if drift:
                        print("locally modified or missing: " + ", ".join(drift))
                    return 1

                current_targets = set(package_files)
                obsolete = obsolete_baselines(project, old_snapshot, current_targets)
                obsolete.extend(retired_config_paths(project, manifest, old_snapshot, current_targets))
                planned: list[tuple[dict, Path, bytes]] = []
                for entry, source in planned_sources:
                    target = safe_path(project, entry["target"])
                    if target.exists() and not target.is_file():
                        raise ValueError(f"target is not a regular file: {entry['target']}")
                    data = source.read_bytes()
                    if target.is_file():
                        current_mode = bool(target.stat().st_mode & stat.S_IXUSR)
                        same_content = digest(target.read_bytes()) == digest(data)
                        same_mode = executable_mode_matches(current_mode, entry["executable"])
                        if same_content and same_mode:
                            continue
                    planned.append((entry, target, data))

                source_revision = revision(package)
                snapshot = {"schema": 1, "source_repository": "https://github.com/qubit-ltd/rs-infra-tools", "source_revision": source_revision, "package_sha256": package_digest, "config_sources": config_versions, "files": package_files}
                snapshot_data = json.dumps(snapshot, indent=2, sort_keys=True) + "\n"
                snap_path = safe_path(project, SNAPSHOT.as_posix())
                if snap_path.exists() and not snap_path.is_file():
                    raise ValueError(f"target is not a regular file: {SNAPSHOT.as_posix()}")
                snapshot_changed = (
                    not snap_path.is_file()
                    or digest(snap_path.read_bytes()) != digest(snapshot_data.encode())
                    or bool(snap_path.stat().st_mode & stat.S_IXUSR)
                )
                changed_targets = [entry["target"] for entry, _, _ in planned]
                if snapshot_changed:
                    changed_targets.append(SNAPSHOT.as_posix())
                if not changed_targets and not obsolete:
                    print("bootstrap files are already up to date")
                    return 0

                print(f"rs-infra-tools {source_revision} will update {len(changed_targets)} paths:")
                for target in changed_targets:
                    print(f"  {target}")
                for path in obsolete:
                    print(f"  remove {path.relative_to(project)}")
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
                for path in obsolete:
                    path.unlink()
                if snapshot_changed:
                    snap_path.parent.mkdir(parents=True, exist_ok=True)
                    snap_path.write_text(snapshot_data, encoding="utf-8")
                    os.chmod(snap_path, 0o644)
                print("bootstrap files updated")
                return 0
        except (OSError, ValueError, json.JSONDecodeError, subprocess.CalledProcessError) as error:
            print(f"error: {error}", file=sys.stderr)
            return 2
    finally:
        cleanup_downloaded_package()


if __name__ == "__main__":
    raise SystemExit(main())
