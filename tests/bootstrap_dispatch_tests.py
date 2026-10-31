from __future__ import annotations

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


REPO = Path(__file__).resolve().parents[1]
BOOTSTRAP = REPO / "assets/project-bootstrap/files/.infra/bootstrap.sh"


class BootstrapDispatchTests(unittest.TestCase):
    def test_stable_update_script_initializes_project_without_infra(self) -> None:
        with tempfile.TemporaryDirectory(prefix="stable-update-initialize-") as temp:
            root = Path(temp)
            project = root / "project"
            project.mkdir()
            updater = project / "update-infra.sh"
            shutil.copyfile(REPO / "update-infra.sh", updater)
            updater.chmod(0o755)
            bin_dir = root / "bin"
            bin_dir.mkdir()
            git = bin_dir / "git"
            git.write_text(
                "#!/usr/bin/env bash\n"
                'if [[ "$1" == clone ]]; then\n'
                '  destination="${@: -1}"\n'
                '  mkdir -p "$destination/assets/project-bootstrap"\n'
                '  touch "$destination/assets/project-bootstrap/sync.py"\n'
                'elif [[ "$1" == -C ]]; then\n'
                '  printf "0123456789abcdef0123456789abcdef01234567\\n"\n'
                'else exit 2; fi\n'
            )
            git.chmod(0o755)
            python = bin_dir / "python-stub"
            call_log = root / "python-args"
            python.write_text(
                "#!/usr/bin/env bash\n"
                'if [[ "${1:-}" == -c ]]; then exit 0; fi\n'
                'printf "%s\\n" "$@" > "$PYTHON_CALL_LOG"\n'
            )
            python.chmod(0o755)
            cargo = bin_dir / "cargo"
            cargo_log = root / "cargo-args"
            cargo.write_text(
                "#!/usr/bin/env bash\n"
                'printf "%s\\n" "$@" > "$CARGO_CALL_LOG"\n'
            )
            cargo.chmod(0o755)
            env = os.environ | {
                "PATH": f"{bin_dir}:{os.environ['PATH']}",
                "TMPDIR": str(root),
                "RS_INFRA_PYTHON": str(python),
                "PYTHON_CALL_LOG": str(call_log),
                "CARGO_CALL_LOG": str(cargo_log),
            }
            for option in ([], ["--dry-run"], ["--status"]):
                with self.subTest(option=option):
                    cargo_log.unlink(missing_ok=True)
                    result = subprocess.run(
                        ["bash", str(updater), *option], env=env,
                        capture_output=True, text=True, timeout=3,
                    )
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    args = call_log.read_text().splitlines()
                    self.assertTrue(args[0].endswith("/assets/project-bootstrap/sync.py"))
                    self.assertIn("--project-root", args)
                    self.assertEqual(args[args.index("--project-root") + 1], str(project))
                    self.assertIn("--package-root", args)
                    self.assertEqual(args[-len(option):] if option else [], option)
                    self.assertNotIn("--yes", args)
                    self.assertEqual(list(root.glob("rs-infra-bootstrap.*")), [])
                    if option:
                        self.assertFalse(cargo_log.exists())
                    else:
                        self.assertTrue(cargo_log.is_file())
                        self.assertEqual(
                            cargo_log.read_text().splitlines()[-4:],
                            ["--", "prewarm", "--project", str(project)],
                        )

    def test_stable_update_script_delegates_existing_project_to_bootstrap(self) -> None:
        with tempfile.TemporaryDirectory(prefix="stable-update-delegate-") as temp:
            root = Path(temp)
            project = root / "project"
            (project / ".infra").mkdir(parents=True)
            updater = project / "update-infra.sh"
            shutil.copyfile(REPO / "update-infra.sh", updater)
            updater.chmod(0o755)
            call_log = root / "bootstrap-args"
            bootstrap = project / ".infra/bootstrap.sh"
            bootstrap.write_text(
                "#!/usr/bin/env bash\n"
                'printf "%s\\n" "$@" > "$BOOTSTRAP_CALL_LOG"\n'
            )
            bootstrap.chmod(0o755)
            for option in ([], ["--dry-run"], ["--status"]):
                result = subprocess.run(
                    ["bash", str(updater), *option],
                    env=os.environ | {"BOOTSTRAP_CALL_LOG": str(call_log)},
                    capture_output=True, text=True, timeout=3,
                )
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                args = call_log.read_text().splitlines()
                self.assertEqual(args[0], ".infra/bin/bootstrap-update.sh")
                self.assertEqual(args[1:], option)
            checker = project / ".infra/bootstrap-check.py"
            checker.write_text("# local checker fixture\n")
            python = root / "python-stub"
            python.write_text(
                "#!/usr/bin/env bash\n"
                'if [[ "${1:-}" == -c ]]; then exit 0; fi\n'
                'printf "%s\\n" "$@" > "$PYTHON_CALL_LOG"\n'
            )
            python.chmod(0o755)
            result = subprocess.run(
                ["bash", str(updater), "--check"],
                env=os.environ | {"RS_INFRA_PYTHON": str(python), "PYTHON_CALL_LOG": str(call_log)},
                capture_output=True, text=True, timeout=3,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(call_log.read_text().splitlines(), [str(checker)])

    def test_manager_repository_update_uses_full_bootstrap_sync(self) -> None:
        with tempfile.TemporaryDirectory(prefix="stable-update-manager-") as temp:
            root = Path(temp)
            project = root / "project"
            (project / ".infra").mkdir(parents=True)
            (project / "Cargo.toml").write_text(
                '[package]\nname = "qubit-infra-tools"\nversion = "0.1.0"\n'
            )
            updater = project / "update-infra.sh"
            shutil.copyfile(REPO / "update-infra.sh", updater)
            call_log = root / "bootstrap-args"
            bootstrap = project / ".infra/bootstrap.sh"
            bootstrap.write_text(
                "#!/usr/bin/env bash\n"
                'printf "%s\\n" "$@" > "$BOOTSTRAP_CALL_LOG"\n'
            )
            bootstrap.chmod(0o755)
            for option in ([], ["--dry-run"], ["--status"]):
                with self.subTest(option=option):
                    result = subprocess.run(
                        ["bash", str(updater), *option],
                        env=os.environ | {"BOOTSTRAP_CALL_LOG": str(call_log)},
                        capture_output=True, text=True, timeout=3,
                    )
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertEqual(
                        call_log.read_text().splitlines(),
                        [".infra/bin/bootstrap-update.sh", *option],
                    )

    def test_stable_update_check_reports_uninitialized_project(self) -> None:
        with tempfile.TemporaryDirectory(prefix="stable-update-check-") as temp:
            project = Path(temp)
            updater = project / "update-infra.sh"
            shutil.copyfile(REPO / "update-infra.sh", updater)
            result = subprocess.run(
                ["bash", str(updater), "--check"], capture_output=True, text=True, timeout=3
            )
            self.assertEqual(result.returncode, 1)
            self.assertIn("not initialized", result.stderr)

    def test_bootstrap_update_route_runs_manager_sync_source(self) -> None:
        with tempfile.TemporaryDirectory(prefix="bootstrap-update-route-") as temp:
            root = Path(temp)
            project = root / "project"
            shared = root / "shared"
            package = shared / "assets/project-bootstrap"
            package.mkdir(parents=True)
            call_log = root / "sync-args"
            (package / "sync.py").write_text(
                "import os, sys\n"
                "with open(os.environ['SYNC_CALL_LOG'], 'w', encoding='utf-8') as stream:\n"
                "    stream.write('\\n'.join(sys.argv[1:]))\n"
            )
            manager = root / "manager"
            manager.write_text("#!/usr/bin/env bash\nexit 99\n")
            manager.chmod(0o755)
            env = os.environ | {
                "RS_INFRA_PROJECT_ROOT": str(project),
                "RS_INFRA_SHARED_ROOT": str(shared),
                "RS_INFRA_TOOLS_BIN": str(manager),
                "RS_INFRA_CACHE_DIR": str(root / "cache"),
                "SYNC_CALL_LOG": str(call_log),
            }
            for bootstrap in (
                REPO / "assets/bootstrap.sh",
                REPO / ".infra/bootstrap.sh",
                BOOTSTRAP,
            ):
                prewarm_log = root / "manager-args"
                manager.write_text(
                    "#!/usr/bin/env bash\n"
                    'printf "%s\\n" "$@" > "$MANAGER_CALL_LOG"\n'
                )
                manager.chmod(0o755)
                for option in ([], ["--dry-run"], ["--status"], ["--configs-only"]):
                    with self.subTest(bootstrap=bootstrap, option=option):
                        prewarm_log.unlink(missing_ok=True)
                        result = subprocess.run(
                            ["bash", str(bootstrap), ".infra/bin/bootstrap-update.sh", *option],
                            env=env | {"MANAGER_CALL_LOG": str(prewarm_log)},
                            capture_output=True, text=True, timeout=3,
                        )
                        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                        self.assertEqual(
                            call_log.read_text().splitlines(),
                            ["--project-root", str(project), "--package-root", str(package), *option],
                        )
                        if "--dry-run" in option or "--status" in option:
                            self.assertFalse(prewarm_log.exists())
                        else:
                            self.assertEqual(
                                prewarm_log.read_text().splitlines(),
                                ["prewarm", "--project", str(project)],
                            )

    def test_infra_tool_wrapper_runs_cached_platform_binary(self) -> None:
        with tempfile.TemporaryDirectory(prefix="bootstrap-platform-") as temp:
            root = Path(temp)
            project = root / "project"
            scripts = project / ".infra/bin"
            scripts.mkdir(parents=True)
            keep = project / "target/debug/keep"
            keep.parent.mkdir(parents=True)
            keep.write_text("original project artifact")
            wrapper = scripts / "infra-tool.sh"
            shutil.copyfile(REPO / ".infra/bin/infra-tool.sh", wrapper)
            bootstrap = project / ".infra/bootstrap.sh"
            shutil.copyfile(REPO / ".infra/bootstrap.sh", bootstrap)
            bootstrap.chmod(0o755)
            bin_dir = root / "bin"
            bin_dir.mkdir()
            call_log = root / "calls"
            platform = bin_dir / "rs-infra-platform"
            platform.write_text(
                "#!/usr/bin/env bash\n"
                'printf "%s\\n" "$@" > "$CALL_LOG"\n'
                "exit 23\n"
            )
            platform.chmod(0o755)
            manager = root / "manager"
            manager.write_text("#!/usr/bin/env bash\nexit 99\n")
            manager.chmod(0o755)
            env = os.environ | {
                "RS_INFRA_PROJECT_ROOT": str(project),
                "RS_INFRA_SHARED_ROOT": str(REPO),
                "RS_INFRA_TOOLS_BIN": str(manager),
                "RS_INFRA_BIN_DIR": str(bin_dir),
                "RS_INFRA_CACHE_DIR": str(root / "cache"),
                "CALL_LOG": str(call_log),
            }
            args = ["--project", str(project), "check"]
            result = subprocess.run(
                ["bash", str(wrapper), "rs-infra-platform", *args],
                env=env, capture_output=True, text=True, timeout=3,
            )
            self.assertEqual(result.returncode, 23, result.stderr)
            self.assertEqual(call_log.read_text().splitlines(), args)
            self.assertEqual(keep.read_text(), "original project artifact")
            rejected = subprocess.run(
                ["bash", str(wrapper), "rs-infra-unknown"],
                env=env, capture_output=True, text=True, timeout=3,
            )
            self.assertEqual(rejected.returncode, 2, rejected.stderr)
            self.assertIn("unknown infra tool 'rs-infra-unknown'", rejected.stderr)
            self.assertEqual(call_log.read_text().splitlines(), args)

    def test_platform_runtime_failure_preserves_original_target(self) -> None:
        with tempfile.TemporaryDirectory(prefix="platform-cleanup-boundary-") as temp:
            root = Path(temp)
            project = root / "project"
            keep = project / "target/debug/keep"
            keep.parent.mkdir(parents=True)
            env = os.environ | {
                "RS_INFRA_PROJECT_ROOT": str(project),
                "RS_INFRA_SHARED_ROOT": str(REPO),
                "RS_INFRA_TOOLS_BIN": str(root / "missing-manager"),
                "RS_INFRA_BIN_DIR": str(root / "empty-bin"),
            }
            env.pop("RS_INFRA_CLEANUP_OWNER_PID", None)
            for tool in ("rs-infra-platform", "rs-infra-style"):
                with self.subTest(tool=tool):
                    keep.parent.mkdir(parents=True, exist_ok=True)
                    keep.write_text("original project artifact")
                    result = subprocess.run(
                        ["bash", str(REPO / ".infra/lib/infra-tool.sh"), tool,
                         "--project", str(project), "check", "--no-cleanup"],
                        env=env, capture_output=True, text=True, timeout=3,
                    )
                    self.assertEqual(result.returncode, 1, result.stderr)
                    self.assertIn("runtime is unavailable", result.stderr)
                    if tool == "rs-infra-platform":
                        self.assertTrue(keep.is_file(), result.stdout)
                        self.assertEqual(keep.read_text(), "original project artifact")
                    else:
                        self.assertFalse(keep.exists(), "legacy cleanup must remain active")

    def test_ci_check_builds_declared_node_test_worker_before_rust_ci(self) -> None:
        with tempfile.TemporaryDirectory(prefix="bootstrap-ci-worker-") as temp:
            root = Path(temp)
            project = root / "project"
            bin_dir = root / "bin"
            script_dir = project / ".infra/bin"
            script_dir.mkdir(parents=True)
            bin_dir.mkdir()
            shutil.copyfile(REPO / "assets/project-bootstrap/files/.infra/bin/ci-check.sh", script_dir / "ci-check.sh")
            (script_dir / "ci-check.sh").chmod(0o755)
            (project / ".infra/bootstrap.sh").write_text(
                "#!/usr/bin/env bash\n"
                'printf "bootstrap:%s\\n" "$*" >> "$CALL_LOG"\n'
            )
            (project / ".infra/bootstrap.sh").chmod(0o755)
            worker = project / "worker"
            worker.mkdir()
            (worker / "package.json").write_text(
                '{"scripts":{"build:test-worker":"build fixture"}}\n'
            )
            calls = root / "calls"
            for executable, label in (("node", "node"), ("corepack", "corepack")):
                stub = bin_dir / executable
                stub.write_text(
                    "#!/usr/bin/env bash\n"
                    f'printf "{label}:%s:%s\\n" "$PWD" "$*" >> "$CALL_LOG"\n'
                )
                stub.chmod(0o755)

            result = subprocess.run(
                ["bash", str(script_dir / "ci-check.sh"), "--dry-run"],
                env=os.environ | {"PATH": f"{bin_dir}:{os.environ['PATH']}", "CALL_LOG": str(calls)},
                capture_output=True,
                text=True,
                timeout=3,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(
                calls.read_text().splitlines(),
                [
                    f"corepack:{worker}:pnpm install --frozen-lockfile --ignore-scripts",
                    f"corepack:{worker}:pnpm run build:test-worker",
                    "bootstrap:.infra/bin/ci-check.sh --dry-run",
                ],
            )

    def test_alignment_runs_strict_clippy_and_propagates_failure(self) -> None:
        with tempfile.TemporaryDirectory(prefix="bootstrap-align-clippy-") as temp:
            root = Path(temp)
            project = root / "project"
            shared = root / "shared"
            project.mkdir()
            shared.mkdir()
            call_log = root / "calls"
            manager = root / "manager"
            manager.write_text(
                "#!/bin/sh\n"
                'printf "%s\\n" "$*" >> "$CALL_LOG"\n'
                'case "$*" in\n'
                '  *rs-infra-ci*) exit "${CLIPPY_STATUS:-0}" ;;\n'
                '  *) exit "${STYLE_STATUS:-0}" ;;\n'
                'esac\n'
            )
            manager.chmod(0o755)
            env = os.environ | {
                "RS_INFRA_PROJECT_ROOT": str(project),
                "RS_INFRA_SHARED_ROOT": str(shared),
                "RS_INFRA_TOOLS_BIN": str(manager),
                "RS_INFRA_CACHE_DIR": str(root / "cache"),
                "CALL_LOG": str(call_log),
            }
            scripts = (BOOTSTRAP, REPO / ".infra/bootstrap.sh", REPO / "assets/bootstrap.sh")
            for bootstrap in scripts:
                for style_status, clippy_status, args in (
                    (0, 0, []), (0, 23, []), (17, 0, []),
                    (0, 23, ["--dry-run"]), (0, 23, ["--help"]),
                ):
                    with self.subTest(script=bootstrap, style=style_status, clippy=clippy_status, args=args):
                        call_log.write_text("")
                        result = subprocess.run(
                            ["bash", str(bootstrap), ".infra/bin/align-ci.sh", *args],
                            env=env | {"STYLE_STATUS": str(style_status), "CLIPPY_STATUS": str(clippy_status)},
                            capture_output=True, text=True, timeout=3,
                        )
                        expected_status = style_status or (0 if args else clippy_status)
                        self.assertEqual(result.returncode, expected_status, result.stderr)
                        calls = call_log.read_text().splitlines()
                        self.assertIn("rs-infra-style", calls[0])
                        if style_status or args:
                            self.assertEqual(len(calls), 1)
                        else:
                            self.assertEqual(len(calls), 2)
                            self.assertEqual(
                                calls[1],
                                f"latest --project {project} --tool rs-infra-ci -- "
                                f"--project {project} --only clippy check",
                            )

    def test_entrypoints_dispatch_without_reentering_bootstrap(self) -> None:
        with tempfile.TemporaryDirectory(prefix="bootstrap-dispatch-") as temp:
            root = Path(temp)
            project = root / "project"
            shared = root / "shared"
            manager = root / "manager"
            call_log = root / "manager-args.txt"

            project_bootstrap = project / ".infra/bootstrap.sh"
            shared_bootstrap = shared / ".infra/bootstrap.sh"
            project_bootstrap.parent.mkdir(parents=True)
            shared_bootstrap.parent.mkdir(parents=True)
            shutil.copyfile(BOOTSTRAP, project_bootstrap)
            shutil.copyfile(BOOTSTRAP, shared_bootstrap)
            project_bootstrap.chmod(0o755)
            shared_bootstrap.chmod(0o755)

            shared_bin = shared / ".infra/bin"
            shared_bin.mkdir(parents=True)
            for entrypoint in (
                "align-ci", "ci-check", "coverage", "dependency-update",
                "infra-tool", "prepare-local-path-dependencies",
                "project-ci-check", "style-check",
            ):
                shared_entrypoint = shared_bin / f"{entrypoint}.sh"
                shared_entrypoint.write_text(
                    "#!/usr/bin/env bash\n"
                    "set -euo pipefail\n"
                    'root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)\n'
                    f'exec "$root/.infra/bootstrap.sh" .infra/bin/{entrypoint}.sh "$@"\n'
                )
                shared_entrypoint.chmod(0o755)

            shared_lib = shared / ".infra/lib"
            shared_lib.mkdir(parents=True)
            for name in (
                "infra-tool",
                "prepare-local-path-dependencies",
                "dependency-update",
                "coverage-report",
            ):
                helper = shared_lib / f"{name}.sh"
                helper.write_text(
                    "#!/usr/bin/env bash\n"
                    "set -euo pipefail\n"
                    f'printf \'{name}\\n\' >> "$CALL_LOG"\n'
                    'if [ "$#" -gt 0 ]; then printf \'%s\\n\' "$@" >> "$CALL_LOG"; fi\n'
                )
                helper.chmod(0o755)

            (shared_lib / "cleanup-build-artifacts.sh").write_text(
                "cleanup_build_artifacts() {\n"
                "  local task_status=$?\n"
                "  rm -f -- \"$project_root/target/ci-artifact\"\n"
                "  printf 'cleanup:%s\\n' \"$task_status\" >> \"$CALL_LOG\"\n"
                "  return \"$task_status\"\n"
                "}\n"
                "if [ -z \"${RS_INFRA_CLEANUP_OWNER_PID:-}\" ]; then\n"
                "  export RS_INFRA_CLEANUP_OWNER_PID=$$\n"
                "  trap cleanup_build_artifacts EXIT\n"
                "fi\n"
            )

            manager.write_text(
                "#!/usr/bin/env bash\n"
                "set -euo pipefail\n"
                'printf \'%s\\n\' "$@" >> "$CALL_LOG"\n'
                'exit "${MANAGER_EXIT_STATUS:-0}"\n'
            )
            manager.chmod(0o755)

            env = os.environ.copy()
            env.update(
                RS_INFRA_PROJECT_ROOT=str(project),
                RS_INFRA_SHARED_ROOT=str(shared),
                RS_INFRA_TOOLS_BIN=str(manager),
                RS_INFRA_CACHE_DIR=str(root / "cache"),
                CALL_LOG=str(call_log),
            )
            cases = {
                "align-ci": [
                    "latest",
                    "--project",
                    str(project),
                    "--tool",
                    "rs-infra-style",
                    "--",
                    "--project",
                    str(project),
                    "fix",
                    "--dry-run",
                ],
                "ci-check": ["latest", "--project", str(project), "--tool", "rs-infra-ci", "--", "--project", str(project), "--dry-run", "check"],
                "coverage": ["latest", "--project", str(project), "--tool", "rs-infra-coverage", "--", "--project", str(project), "collect", "--dry-run"],
                "style-check": ["latest", "--project", str(project), "--tool", "rs-infra-style", "--", "--project", str(project), "check", "--dry-run"],
            }
            for entrypoint, expected in cases.items():
                call_log.unlink(missing_ok=True)
                if entrypoint == "ci-check":
                    (project / "target").mkdir(parents=True, exist_ok=True)
                    (project / "target/ci-artifact").touch()
                result = subprocess.run(
                    ["bash", str(project_bootstrap), f".infra/bin/{entrypoint}.sh", "--dry-run"],
                    env=env,
                    text=True,
                    capture_output=True,
                    timeout=3,
                )
                self.assertEqual(result.returncode, 0, f"{entrypoint}: {result.stdout}{result.stderr}")
                expected_log = expected
                if entrypoint in {"align-ci", "ci-check", "coverage", "style-check"}:
                    expected_log = ["prepare-local-path-dependencies", *expected]
                if entrypoint == "coverage":
                    expected_log.append("coverage-report")
                if entrypoint == "ci-check":
                    expected_log.append("cleanup:0")
                    self.assertFalse((project / "target/ci-artifact").exists())
                self.assertEqual(call_log.read_text().splitlines(), expected_log, entrypoint)

            call_log.unlink(missing_ok=True)
            (project / "target").mkdir(parents=True, exist_ok=True)
            failed_artifact = project / "target/ci-artifact"
            failed_artifact.touch()
            failed_env = env | {"MANAGER_EXIT_STATUS": "23"}
            failed_result = subprocess.run(
                ["bash", str(project_bootstrap), ".infra/bin/ci-check.sh"],
                env=failed_env,
                text=True,
                capture_output=True,
                timeout=3,
            )
            self.assertEqual(failed_result.returncode, 23, failed_result.stdout + failed_result.stderr)
            self.assertFalse(failed_artifact.exists())
            self.assertEqual(call_log.read_text().splitlines()[-1], "cleanup:23")

            call_log.unlink(missing_ok=True)
            result = subprocess.run(
                ["bash", str(project_bootstrap), ".infra/bin/dependency-update.sh", "--check"],
                env=env,
                text=True,
                capture_output=True,
                timeout=3,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(call_log.read_text().splitlines(), ["prepare-local-path-dependencies", "latest", "--project", str(project), "--tool", "rs-infra-dependency", "--", "--project", str(project), "--recursive", "check"])

            call_log.unlink(missing_ok=True)
            result = subprocess.run(
                ["bash", str(project_bootstrap), ".infra/bin/dependency-update.sh"],
                env=env,
                text=True,
                capture_output=True,
                timeout=3,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(
                call_log.read_text().splitlines(),
                [
                    "prepare-local-path-dependencies",
                    "latest", "--project", str(project), "--tool", "rs-infra-dependency", "--", "--project", str(project), "--recursive", "sync",
                    "latest", "--project", str(project), "--tool", "rs-infra-dependency", "--", "--project", str(project), "--recursive", "check",
                ],
            )

            local = shared / ".infra/lib"
            local_cases = {
                "infra-tool": ("infra-tool", ["rs-infra-style", "check"]),
                "prepare-local-path-dependencies": ("prepare-local-path-dependencies", ["--dry-run"]),
            }
            for entrypoint, (helper, args) in local_cases.items():
                call_log.unlink(missing_ok=True)
                result = subprocess.run(
                    ["bash", str(project_bootstrap), f".infra/bin/{entrypoint}.sh", *args],
                    env=env,
                    text=True,
                    capture_output=True,
                    timeout=3,
                )
                self.assertEqual(result.returncode, 0, f"{entrypoint}: {result.stdout}{result.stderr}")
                self.assertEqual(call_log.read_text().splitlines(), [helper, *args], entrypoint)

            # The project hook is optional; without one it succeeds, and it invokes the local override when present.
            call_log.unlink(missing_ok=True)
            result = subprocess.run(
                ["bash", str(project_bootstrap), ".infra/bin/project-ci-check.sh"],
                env=env,
                text=True,
                capture_output=True,
                timeout=3,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            hook = project / ".infra/project-ci-check.local.sh"
            hook.parent.mkdir(parents=True, exist_ok=True)
            hook.write_text('#!/usr/bin/env bash\nprintf "project-hook\\n%s\\n" "$@" >> "$CALL_LOG"\n')
            hook.chmod(0o755)
            result = subprocess.run(
                ["bash", str(project_bootstrap), ".infra/bin/project-ci-check.sh", "--ci-arg"],
                env=env,
                text=True,
                capture_output=True,
                timeout=3,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(call_log.read_text().splitlines(), ["project-hook", "--ci-arg"])


if __name__ == "__main__":
    unittest.main()
