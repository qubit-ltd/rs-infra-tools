from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


REPO = Path(__file__).resolve().parents[1]
PACKAGE = REPO / "assets/project-bootstrap"
SPEC = importlib.util.spec_from_file_location("project_bootstrap_sync", PACKAGE / "sync.py")
sync = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
sys.modules[SPEC.name] = sync
SPEC.loader.exec_module(sync)


class ProjectBootstrapSyncTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.project = Path(self.temp.name) / "project"
        self.project.mkdir()
        self.package = Path(self.temp.name) / "package"
        self.package.mkdir()
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

    def test_repeated_sync_is_idempotent(self) -> None:
        first = self.run_sync("--yes")
        snapshot = (self.project / ".infra/bootstrap-source.json").read_bytes()
        second = self.run_sync("--yes")
        self.assertEqual(first.returncode, 0, first.stderr)
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertEqual((self.project / ".infra/bootstrap-source.json").read_bytes(), snapshot)


if __name__ == "__main__":
    unittest.main()
