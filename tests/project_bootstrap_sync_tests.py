from __future__ import annotations

import importlib.util
import io
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from unittest import mock


REPO = Path(__file__).resolve().parents[1]
PACKAGE = REPO / "assets/project-bootstrap"
SPEC = importlib.util.spec_from_file_location("project_bootstrap_sync", PACKAGE / "sync.py")
sync = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
sys.modules[SPEC.name] = sync
SPEC.loader.exec_module(sync)


class ProjectBootstrapSyncTests(unittest.TestCase):
    def test_repository_manifest_installs_manager_defaults_and_declares_retirement(self) -> None:
        manifest = sync.load_manifest(PACKAGE)
        self.assertIn(
            {"name": "rs-infra-tools", "directory": "tools", "local": "manager"},
            manifest["config_sources"],
        )
        self.assertIn(".infra/ci/defaults.toml", manifest["retired_targets"])
        with mock.patch.object(sync.subprocess, "run", side_effect=AssertionError("unexpected clone")):
            entries, versions = sync.config_entries(
                {"files": [], "config_sources": [
                    {"name": "rs-infra-tools", "directory": "tools", "local": "manager"}
                ]}, Path(tempfile.gettempdir()) / "unused-checkouts", PACKAGE
            )
        self.assertEqual(entries[0][0]["target"], ".infra/tools/defaults.toml")
        self.assertIn("rs-infra-tools", versions)

    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.project = Path(self.temp.name) / "project"
        self.project.mkdir()
        self.package = Path(self.temp.name) / "manager/assets/project-bootstrap"
        self.package.mkdir(parents=True)
        (self.package / "manifest.json").write_text(json.dumps({"version": 1, "files": [
            {"source": "source/tool.sh", "target": ".infra/tool.sh", "executable": True}
        ]}))
        (self.package / "source").mkdir()
        (self.package / "source/tool.sh").write_text("#!/bin/sh\necho upstream\n")

    def run_sync(self, *args: str, input_text: str | None = None) -> subprocess.CompletedProcess[str]:
        return subprocess.run([
            sys.executable, str(PACKAGE / "sync.py"), "--project-root", str(self.project),
            "--package-root", str(self.package), *args,
        ], input=input_text, text=True, capture_output=True)

    def test_noninteractive_requires_yes(self) -> None:
        result = self.run_sync()
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertFalse((self.project / ".infra/tool.sh").exists())

    def test_dry_run_does_not_write(self) -> None:
        result = self.run_sync("--dry-run")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse((self.project / ".infra/tool.sh").exists())

    def test_yes_overwrites_and_records_mode_and_hash(self) -> None:
        target = self.project / ".infra/tool.sh"
        target.parent.mkdir()
        target.write_text("locally changed")
        result = self.run_sync("--yes")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(target.read_text(), "#!/bin/sh\necho upstream\n")
        self.assertTrue(target.stat().st_mode & 0o100)
        snapshot, drift = sync.project_state(self.project)
        self.assertFalse(drift)
        self.assertEqual(snapshot["files"][".infra/tool.sh"]["executable"], True)
        target.write_text("modified after installation")
        _, drift = sync.project_state(self.project)
        self.assertEqual(drift, [".infra/tool.sh"])

    def test_path_escape_and_duplicate_targets_are_rejected(self) -> None:
        manifest = self.package / "manifest.json"
        manifest.write_text(json.dumps({"version": 1, "files": [
            {"source": "source/tool.sh", "target": "../outside", "executable": True}
        ]}))
        with self.assertRaises(ValueError):
            sync.load_manifest(self.package)
        manifest.write_text(json.dumps({"version": 1, "files": [
            {"source": "source/tool.sh", "target": "a", "executable": True},
            {"source": "source/tool.sh", "target": "a", "executable": True}
        ]}))
        with self.assertRaises(ValueError):
            sync.load_manifest(self.package)

    def test_selected_profile_is_loaded_from_project_config(self) -> None:
        manifest = json.loads((self.package / "manifest.json").read_text())
        (self.package / "source/config.toml").write_text("managed = true\n")
        manifest["profiles"] = {"shared-default": [
            {"source": "source/config.toml", "target": ".infra/config.toml", "executable": False}
        ]}
        (self.package / "manifest.json").write_text(json.dumps(manifest))
        (self.project / ".infra").mkdir()
        (self.project / ".infra/bootstrap-update.toml").write_text('profiles = ["shared-default"]\n')
        selected = sync.selected_entries(sync.load_manifest(self.package), self.project)
        self.assertEqual([entry["target"] for entry in selected], [".infra/tool.sh", ".infra/config.toml"])

    def test_symlink_escape_is_rejected(self) -> None:
        outside = Path(self.temp.name) / "outside"
        outside.mkdir()
        (self.project / ".infra").symlink_to(outside, target_is_directory=True)
        with self.assertRaises(ValueError):
            sync.safe_path(self.project, ".infra/tool.sh")

    def test_windows_mode_check_ignores_unrepresentable_execute_bit(self) -> None:
        self.assertTrue(sync.executable_mode_matches(False, True, windows=True))
        self.assertFalse(sync.executable_mode_matches(False, True, windows=False))

    def test_downloaded_source_cleanup_requires_its_marker(self) -> None:
        temp_root = Path(self.temp.name) / "rs-infra-bootstrap.fixture"
        temp_root.mkdir()
        marker = temp_root / sync.TEMP_MARKER
        marker.write_text("rs-infra-bootstrap-temp-v1\n")
        (temp_root / "source").mkdir()
        sync.os.environ["RS_INFRA_BOOTSTRAP_TEMP_ROOT"] = str(temp_root)
        sync.cleanup_downloaded_package()
        self.assertFalse(temp_root.exists())
        self.assertNotIn("RS_INFRA_BOOTSTRAP_TEMP_ROOT", sync.os.environ)

    def test_repeated_sync_is_idempotent(self) -> None:
        first = self.run_sync("--yes")
        snapshot = (self.project / ".infra/bootstrap-source.json").read_bytes()
        self.assertEqual(first.returncode, 0, first.stderr)
        output = io.StringIO()
        argv = [
            "sync.py", "--project-root", str(self.project),
            "--package-root", str(self.package), "--yes",
        ]
        with mock.patch.object(sys, "argv", argv), mock.patch.object(sync.os, "replace", wraps=sync.os.replace) as replace_file:
            with redirect_stdout(output):
                second = sync.main()
        self.assertEqual(second, 0)
        self.assertEqual(replace_file.call_count, 0)
        self.assertIn("already up to date", output.getvalue())
        self.assertEqual((self.project / ".infra/bootstrap-source.json").read_bytes(), snapshot)

    def test_snapshot_check_accepts_git_for_windows_crlf_checkout(self) -> None:
        result = self.run_sync("--yes")
        self.assertEqual(result.returncode, 0, result.stderr)
        target = self.project / ".infra/tool.sh"
        target.write_bytes(target.read_bytes().replace(b"\n", b"\r\n"))
        checker = self.project / ".infra/bootstrap-check.py"
        checker.write_bytes((PACKAGE / "files/.infra/bootstrap-check.py").read_bytes())
        checked = subprocess.run(
            [sys.executable, str(checker)], text=True, capture_output=True
        )
        self.assertEqual(checked.returncode, 0, checked.stdout + checked.stderr)
        self.assertIn("match", checked.stdout)

    def test_package_digest_is_stable_across_crlf_checkout(self) -> None:
        manifest_path = self.package / "manifest.json"
        manifest = json.loads(manifest_path.read_text())
        package_sha = sync.package_state(self.package, manifest)
        manifest_path.write_bytes(manifest_path.read_bytes().replace(b"\n", b"\r\n"))
        source_path = self.package / "source/tool.sh"
        source_path.write_bytes(source_path.read_bytes().replace(b"\n", b"\r\n"))
        self.assertEqual(sync.package_state(self.package, manifest), package_sha)

    def test_shared_config_version_tracks_conf_contents(self) -> None:
        source = Path(self.temp.name) / "source-repo"
        conf = source / "conf"
        conf.mkdir(parents=True)
        (conf / "manifest.json").write_text(json.dumps({"version": 1, "files": [
            {"source": "defaults.toml", "target": "defaults.toml"}
        ]}))
        config = conf / "defaults.toml"
        config.write_text('version = "1"\n')
        manifest = sync.load_manifest(self.package)
        manifest["config_sources"] = [{"name": "rs-infra-ci", "directory": "ci", "repository": str(source)}]

        def copy_checkout(command: list[str], **_kwargs: object) -> subprocess.CompletedProcess[str]:
            shutil.copytree(conf, Path(command[-1]) / "conf")
            return subprocess.CompletedProcess(command, 0)

        with mock.patch.object(sync.subprocess, "run", side_effect=copy_checkout):
            _, first = sync.config_entries(manifest, Path(self.temp.name) / "checkout-1")
            (source / "unrelated.txt").write_text("a later repository commit\n")
            _, same = sync.config_entries(manifest, Path(self.temp.name) / "checkout-2")
            config.write_text('version = "2"\n')
            _, changed = sync.config_entries(manifest, Path(self.temp.name) / "checkout-3")

        self.assertEqual(first, same)
        self.assertNotEqual(first, changed)

    def write_manager_config(self) -> Path:
        manifest = json.loads((self.package / "manifest.json").read_text())
        manifest["config_sources"] = [
            {"name": "rs-infra-tools", "directory": "tools", "local": "manager"}
        ]
        (self.package / "manifest.json").write_text(json.dumps(manifest))
        conf = self.package.parent.parent / "conf"
        conf.mkdir()
        (conf / "manifest.json").write_text(json.dumps({"version": 1, "files": [
            {"source": "defaults.toml", "target": "defaults.toml"}
        ]}))
        defaults = conf / "defaults.toml"
        defaults.write_text('build_toolchain = "1.94.0"\n')
        return defaults

    def test_manager_config_uses_package_checkout_without_cloning(self) -> None:
        defaults = self.write_manager_config()
        manifest = sync.load_manifest(self.package)
        with mock.patch.object(sync.subprocess, "run", side_effect=AssertionError("unexpected clone")):
            entries, first_versions = sync.config_entries(
                manifest, Path(self.temp.name) / "checkout-1", self.package
            )
            defaults.write_text('build_toolchain = "1.95.0"\n')
            _, second_versions = sync.config_entries(
                manifest, Path(self.temp.name) / "checkout-2", self.package
            )
        self.assertEqual(entries[0][0]["target"], ".infra/tools/defaults.toml")
        self.assertNotEqual(first_versions, second_versions)

    def test_manager_config_install_tracks_contents_and_does_not_rewrite_unchanged_file(self) -> None:
        defaults = self.write_manager_config()
        first = self.run_sync("--yes")
        self.assertEqual(first.returncode, 0, first.stderr)
        target = self.project / ".infra/tools/defaults.toml"
        self.assertEqual(target.read_bytes(), defaults.read_bytes())
        snapshot, drift = sync.project_state(self.project)
        self.assertFalse(drift)
        self.assertIn("rs-infra-tools", snapshot["config_sources"])
        first_version = snapshot["config_sources"]["rs-infra-tools"]
        first_inode = target.stat().st_ino
        same = self.run_sync("--yes")
        self.assertEqual(same.returncode, 0, same.stderr)
        self.assertEqual(target.stat().st_ino, first_inode)
        defaults.write_text('build_toolchain = "1.95.0"\n')
        changed = self.run_sync("--yes")
        self.assertEqual(changed.returncode, 0, changed.stderr)
        later, drift = sync.project_state(self.project)
        self.assertFalse(drift)
        self.assertNotEqual(later["config_sources"]["rs-infra-tools"], first_version)
        self.assertEqual(target.read_bytes(), defaults.read_bytes())

    def install_old_defaults(self) -> Path:
        old = self.project / ".infra/ci/defaults.toml"
        old.parent.mkdir(parents=True)
        old.write_text('build_toolchain = "1.94.0"\n')
        result = self.run_sync("--yes")
        self.assertEqual(result.returncode, 0, result.stderr)
        snapshot_path = self.project / ".infra/bootstrap-source.json"
        snapshot = json.loads(snapshot_path.read_text())
        snapshot["files"][".infra/ci/defaults.toml"] = {
            "sha256": sync.digest(old.read_bytes()), "executable": False
        }
        snapshot_path.write_text(json.dumps(snapshot))
        return old

    def enable_defaults_retirement(self) -> None:
        manifest_path = self.package / "manifest.json"
        manifest = json.loads(manifest_path.read_text())
        manifest["retired_targets"] = [".infra/ci/defaults.toml"]
        manifest_path.write_text(json.dumps(manifest))

    def test_retirement_removes_snapshot_owned_old_defaults(self) -> None:
        old = self.install_old_defaults()
        self.write_manager_config()
        self.enable_defaults_retirement()
        result = self.run_sync("--yes")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(old.exists())
        snapshot, drift = sync.project_state(self.project)
        self.assertFalse(drift)
        self.assertNotIn(".infra/ci/defaults.toml", snapshot["files"])
        self.assertTrue((self.project / ".infra/tools/defaults.toml").exists())

    def test_retirement_allows_fresh_project_without_old_defaults(self) -> None:
        self.write_manager_config()
        self.enable_defaults_retirement()
        result = self.run_sync("--yes")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue((self.project / ".infra/tools/defaults.toml").exists())

    def test_retirement_rejects_modified_old_defaults_before_other_writes(self) -> None:
        old = self.install_old_defaults()
        old.write_text("local changes\n")
        self.write_manager_config()
        self.enable_defaults_retirement()
        original_tool = (self.project / ".infra/tool.sh").read_bytes()
        (self.package / "source/tool.sh").write_text("new upstream tool\n")
        result = self.run_sync("--yes")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn(".infra/ci/defaults.toml", result.stderr)
        self.assertEqual(old.read_text(), "local changes\n")
        self.assertEqual((self.project / ".infra/tool.sh").read_bytes(), original_tool)
        self.assertFalse((self.project / ".infra/tools/defaults.toml").exists())

    def test_retirement_rejects_missing_snapshot_owned_old_defaults(self) -> None:
        old = self.install_old_defaults()
        old.unlink()
        self.write_manager_config()
        self.enable_defaults_retirement()
        result = self.run_sync("--yes")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn(".infra/ci/defaults.toml", result.stderr)
        self.assertFalse((self.project / ".infra/tools/defaults.toml").exists())

    def test_retirement_rejects_untracked_old_defaults(self) -> None:
        old = self.install_old_defaults()
        snapshot_path = self.project / ".infra/bootstrap-source.json"
        snapshot = json.loads(snapshot_path.read_text())
        del snapshot["files"][".infra/ci/defaults.toml"]
        snapshot_path.write_text(json.dumps(snapshot))
        self.write_manager_config()
        self.enable_defaults_retirement()
        result = self.run_sync("--yes")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn(".infra/ci/defaults.toml", result.stderr)
        self.assertTrue(old.exists())
        self.assertFalse((self.project / ".infra/tools/defaults.toml").exists())


if __name__ == "__main__":
    unittest.main()
