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
