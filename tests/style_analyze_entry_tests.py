"""Read-only project Analyzer entry and managed installation contracts."""
from __future__ import annotations

import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

REPO = Path(__file__).resolve().parents[1]
FILES = REPO / "assets/project-bootstrap/files"
HELPER = FILES / ".infra/lib/style_analyze.py"


class StyleAnalyzeEntryTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="style-analyze-entry-")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.project = self.root / "project"
        self.project.mkdir()
        subprocess.run(["git", "init", "-q", str(self.project)], check=True)
        self.script = self.project / ".infra/bin/style-analyze.sh"
        self.helper = self.project / ".infra/lib/style_analyze.py"
        for source, target in ((FILES / ".infra/bin/style-analyze.sh", self.script), (HELPER, self.helper)):
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, target)
            target.chmod(0o755)
        binary = self.project / ".infra/style/bin/rs-infra-style"
        binary.parent.mkdir(parents=True)
        binary.write_text("#!/usr/bin/env python3\nimport json,os,pathlib,sys\n"
                          "assert sys.argv[1] == 'analyze',sys.argv\n"
                          "args=sys.argv[2:]; source=pathlib.Path(args[args.index('--files-from')+1])\n"
                          "project=pathlib.Path(args[args.index('--project')+1]); assert project not in source.parents\n"
                          "print(json.dumps({'files':json.loads(source.read_text()),'args':args,'manifest':str(source)}))\n"
                          "sys.exit(int(os.environ.get('TEST_ANALYZER_EXIT','0')))\n")
        binary.chmod(0o755)
        self.binary = binary
        self.metadata = self.project / ".infra/style/tool.revision.json"
        self.metadata.write_text(json.dumps({"schema_version": 1, "tool": "rs-infra-style",
            "revision": "a" * 40, "sha256": hashlib.sha256(binary.read_bytes()).hexdigest()}))
        self.env = dict(os.environ)
        self.env.pop("RUST_STYLE_ANALYZER", None)
        self.env["RS_INFRA_PYTHON"] = sys.executable

    def source(self, name):
        path = self.project / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("// fixture\n")
        return path

    def run_entry(self, *args, env=None):
        return subprocess.run(["bash", str(self.script), *args], cwd=self.root,
                              capture_output=True, text=True, env=env or self.env)

    def test_default_inventory_uses_entry_project_and_external_temporary_manifest(self):
        names = ["src/lib.rs", "other/src/lib.rs", "fuzz/fuzz_targets/input.rs", "fixtures/fail.rs"]
        for name in names + ["vendor/x.rs", "vendored/x.rs", "generated/x.rs", "target/x.rs", ".codex/x.rs", "ignored.rs"]:
            self.source(name)
        (self.project / ".gitignore").write_text("ignored.rs\n/.infra/style/bin/\n")
        subprocess.run(["git", "-C", str(self.project), "add", "src/lib.rs"], check=True)
        before = {str(p): p.read_bytes() for p in self.project.rglob('*') if p.is_file() and '.git' not in p.parts}
        result = self.run_entry("--format", "json")
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(result.stdout)
        self.assertEqual(report["files"], sorted(names))
        self.assertFalse(Path(report["manifest"]).exists())
        after = {str(p): p.read_bytes() for p in self.project.rglob('*') if p.is_file() and '.git' not in p.parts}
        self.assertEqual(before, after)

    def test_help_works_without_installed_binary(self):
        self.binary.unlink()
        result = self.run_entry("--help")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("--files-from", result.stdout)

    def test_describe_returns_verified_identity_without_running_native(self):
        result = self.run_entry("--describe-analyzer")
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(result.stdout)
        self.assertEqual(report["executable"], str(self.binary))
        self.assertEqual(report["sha256"], hashlib.sha256(self.binary.read_bytes()).hexdigest())
        self.assertEqual(report["revision"], "a" * 40)

    def test_missing_and_corrupt_installations_fail_actionably(self):
        for damage in ("metadata", "binary", "hash"):
            with self.subTest(damage=damage):
                original_bin, original_metadata = self.binary.read_bytes(), self.metadata.read_bytes()
                if damage == "metadata": self.metadata.unlink()
                elif damage == "binary": self.binary.unlink()
                else: self.binary.write_bytes(original_bin + b"# altered\n")
                result = self.run_entry("--describe-analyzer")
                self.assertEqual(result.returncode, 2)
                self.assertIn("update-infra.sh", result.stderr)
                self.binary.write_bytes(original_bin); self.binary.chmod(0o755)
                self.metadata.write_bytes(original_metadata)

    def test_explicit_override_preserves_priority_without_managed_metadata(self):
        override = self.root / "override"
        shutil.copyfile(self.binary, override); override.chmod(0o755)
        self.metadata.unlink()
        result = self.run_entry("--describe-analyzer", env=self.env | {"RUST_STYLE_ANALYZER": str(override)})
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(result.stdout)
        self.assertEqual(report["executable"], str(override))
        self.assertIsNone(report["revision"])

    def test_analysis_project_can_be_worker_with_no_local_binary(self):
        worker = self.root / "worker"; worker.mkdir()
        subprocess.run(["git", "init", "-q", str(worker)], check=True)
        (worker / "file.rs").write_text("// worker")
        result = self.run_entry("--project", str(worker))
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(result.stdout)
        self.assertEqual(report["files"], ["file.rs"])
        self.assertEqual(report["args"][1], str(worker))

    def test_native_exit_codes_and_forwarded_flags_are_preserved(self):
        self.source("src/lib.rs")
        for code in (0, 1, 2):
            result = self.run_entry("--repository-readme", "--standards", str(self.root),
                env=self.env | {"TEST_ANALYZER_EXIT": str(code)})
            self.assertEqual(result.returncode, code, result.stderr)
            self.assertIn("--repository-readme", json.loads(result.stdout)["args"])

    def test_explicit_file_list_does_not_enumerate_project(self):
        self.source("one.rs"); self.source("two.rs")
        manifest = self.root / "list.json"; manifest.write_text('["two.rs"]')
        result = self.run_entry("--files-from", str(manifest))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["files"], ["two.rs"])

    def test_source_symlinks_are_rejected(self):
        target = self.root / "external.rs"; target.write_text("// external")
        (self.project / "link.rs").symlink_to(target)
        result = self.run_entry()
        self.assertEqual(result.returncode, 2)
        self.assertIn("symbolic", result.stderr.lower())

    def test_managed_binary_symlinks_are_rejected(self):
        outside = self.root / "native"; shutil.copyfile(self.binary, outside); outside.chmod(0o755)
        self.binary.unlink(); self.binary.symlink_to(outside)
        result = self.run_entry("--describe-analyzer")
        self.assertEqual(result.returncode, 2)
        self.assertIn("symbolic", result.stderr.lower())

    def test_installation_changed_during_native_call_is_not_accepted(self):
        self.source("src/lib.rs")
        text = self.binary.read_text().replace("import json,os,pathlib,sys", "import json,os,pathlib,sys\npathlib.Path(__file__).write_text('# changed')")
        self.binary.write_text(text)
        metadata = json.loads(self.metadata.read_text())
        metadata['sha256'] = hashlib.sha256(self.binary.read_bytes()).hexdigest()
        self.metadata.write_text(json.dumps(metadata))
        result = self.run_entry()
        self.assertEqual(result.returncode, 2)
        self.assertIn('do not match', result.stderr)

    def test_generated_install_artifacts_are_ignored_without_root_rules(self):
        ignore = self.project / '.infra/style/.gitignore'
        shutil.copyfile(FILES / '.infra/style/.gitignore', ignore)
        lock = self.project / '.infra/style/tool.revision.lock'; lock.touch()
        for path in (self.binary, self.metadata, lock):
            result = subprocess.run(['git', '-C', str(self.project), 'check-ignore', '-q', str(path)])
            self.assertEqual(result.returncode, 0, str(path))

    def test_managed_manifest_declares_entry_helper_and_ignore_file(self):
        manifest = json.loads((REPO / "assets/project-bootstrap/manifest.json").read_text())
        installed = {entry['target']: entry for entry in manifest['files']}
        for path in (".infra/bin/style-analyze.sh", ".infra/lib/style_analyze.py", ".infra/style/.gitignore"):
            self.assertIn(path, installed)
        self.assertTrue(installed[".infra/bin/style-analyze.sh"]['executable'])


class LegacyAnalyzerUpgradeTests(unittest.TestCase):
    def test_legacy_hook_skips_preview_configs_and_deferred_callers(self):
        spec = importlib.util.spec_from_file_location('style_sync', REPO / 'assets/project-bootstrap/sync.py')
        sync = importlib.util.module_from_spec(spec); spec.loader.exec_module(sync)
        from types import SimpleNamespace
        from unittest.mock import patch
        args = dict(defer_style_analyzer_install=False, configs_only=False, dry_run=False, status=False)
        files = {name: {} for name in ('.infra/bin/style-analyze.sh', '.infra/lib/style_analyze.py', '.infra/style/.gitignore')}
        for flag in args:
            with self.subTest(flag=flag), patch.object(sync.subprocess, 'run', side_effect=AssertionError('no install')):
                sync.install_analyzer_for_legacy_bootstrap(Path('/project'), Path('/package'), files,
                    SimpleNamespace(**(args | {flag: True})))

    def test_legacy_hook_propagates_manager_failure_and_can_be_retried(self):
        spec = importlib.util.spec_from_file_location('style_sync', REPO / 'assets/project-bootstrap/sync.py')
        sync = importlib.util.module_from_spec(spec); spec.loader.exec_module(sync)
        from types import SimpleNamespace
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); project = root / 'project'; project.mkdir()
            package = root / 'shared/assets/project-bootstrap'; package.mkdir(parents=True)
            runner = root / 'manager'; runner.write_text('#!/bin/sh\nexit 7\n'); runner.chmod(0o755)
            env = dict(RS_INFRA_TOOLS_BIN=str(runner), RS_INFRA_PROJECT_ROOT=str(project),
                       RS_INFRA_SHARED_ROOT=str(root / 'shared'))
            args = SimpleNamespace(defer_style_analyzer_install=False, configs_only=False, dry_run=False, status=False)
            files = {name: {} for name in ('.infra/bin/style-analyze.sh', '.infra/lib/style_analyze.py', '.infra/style/.gitignore')}
            with patch.dict(os.environ, env):
                with self.assertRaises(subprocess.CalledProcessError):
                    sync.install_analyzer_for_legacy_bootstrap(project, package, files, args)
                runner.write_text('#!/bin/sh\nexit 0\n')
                sync.install_analyzer_for_legacy_bootstrap(project, package, files, args)

    def test_old_in_memory_bootstrap_installs_analyzer_in_one_update(self):
        with tempfile.TemporaryDirectory(prefix="legacy-analyzer-update-") as directory:
            root = Path(directory); project = root / "project"; project.mkdir()
            shared = root / "shared"; package = shared / "assets/project-bootstrap"
            shutil.copytree(FILES, package / "files")
            shutil.copyfile(REPO / "assets/project-bootstrap/sync.py", package / "sync.py")
            manifest = json.loads((REPO / "assets/project-bootstrap/manifest.json").read_text())
            manifest["config_sources"] = []; manifest["profiles"] = {}
            (package / "manifest.json").write_text(json.dumps(manifest))
            old = project / ".infra/bootstrap.sh"; old.parent.mkdir(parents=True)
            old.write_text("""#!/usr/bin/env bash
set -euo pipefail
python3 "$RS_INFRA_SHARED_ROOT/assets/project-bootstrap/sync.py" --project-root "$RS_INFRA_PROJECT_ROOT" --package-root "$RS_INFRA_SHARED_ROOT/assets/project-bootstrap" "$@"
"$RS_INFRA_TOOLS_BIN" prewarm --project "$RS_INFRA_PROJECT_ROOT"
""")
            manager = root / "manager"
            manager.write_text("""#!/usr/bin/env python3
import os,pathlib,sys
args=sys.argv[1:];project=pathlib.Path(args[args.index('--project')+1])
with open(os.environ['CALL_LOG'],'a') as f:f.write(' '.join(args)+'\\n')
if '--install-style-analyzer' in args:
 p=project/'.infra/style/bin/rs-infra-style';p.parent.mkdir(parents=True,exist_ok=True);p.write_text('installed')
"""); manager.chmod(0o755)
            env = dict(os.environ, RS_INFRA_PROJECT_ROOT=str(project), RS_INFRA_SHARED_ROOT=str(shared),
                RS_INFRA_TOOLS_BIN=str(manager), RS_INFRA_SOURCE_REVISION="a" * 40, CALL_LOG=str(root / 'calls'))
            result = subprocess.run(['bash', str(old)], env=env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertTrue((project / '.infra/bin/style-analyze.sh').is_file())
            self.assertTrue((project / '.infra/style/bin/rs-infra-style').is_file())
            self.assertIn('--install-style-analyzer', (root / 'calls').read_text())
            self.assertEqual((root / 'calls').read_text().count('--install-style-analyzer'), 1)


if __name__ == "__main__":
    unittest.main()
