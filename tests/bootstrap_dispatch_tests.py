from __future__ import annotations

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


REPO = Path(__file__).resolve().parents[1]
BOOTSTRAP = REPO / "assets/bootstrap.sh"


class BootstrapDispatchTests(unittest.TestCase):
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
            for name in ("infra-tool", "prepare-local-path-dependencies", "dependency-update"):
                helper = shared_lib / f"{name}.sh"
                helper.write_text(
                    "#!/usr/bin/env bash\n"
                    "set -euo pipefail\n"
                    f'printf \'{name}\\n\' >> "$CALL_LOG"\n'
                    'if [ "$#" -gt 0 ]; then printf \'%s\\n\' "$@" >> "$CALL_LOG"; fi\n'
                )
                helper.chmod(0o755)

            manager.write_text(
                "#!/usr/bin/env bash\n"
                "set -euo pipefail\n"
                'printf \'%s\\n\' "$@" >> "$CALL_LOG"\n'
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
                self.assertEqual(call_log.read_text().splitlines(), expected_log, entrypoint)

            call_log.unlink(missing_ok=True)
            result = subprocess.run(
                ["bash", str(project_bootstrap), ".infra/bin/dependency-update.sh", "--check"],
                env=env,
                text=True,
                capture_output=True,
                timeout=3,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(call_log.read_text().splitlines(), ["prepare-local-path-dependencies", "latest", "--project", str(project), "--tool", "rs-infra-dependency", "--", "--project", str(project), "check"])

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
                    "latest", "--project", str(project), "--tool", "rs-infra-dependency", "--", "--project", str(project), "sync",
                    "latest", "--project", str(project), "--tool", "rs-infra-dependency", "--", "--project", str(project), "check",
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
