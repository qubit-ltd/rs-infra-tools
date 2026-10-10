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
    def test_align_ci_dispatches_to_style_fix_without_reentering_bootstrap(self) -> None:
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

            shared_entrypoint = shared / ".infra/bin/align-ci.sh"
            shared_entrypoint.parent.mkdir(parents=True)
            shared_entrypoint.write_text(
                "#!/usr/bin/env bash\n"
                "set -euo pipefail\n"
                'root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)\n'
                'exec "$root/.infra/bootstrap.sh" .infra/bin/align-ci.sh "$@"\n'
            )
            shared_entrypoint.chmod(0o755)

            manager.write_text(
                "#!/usr/bin/env bash\n"
                "set -euo pipefail\n"
                'printf \'%s\\n\' "$@" > "$CALL_LOG"\n'
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
            result = subprocess.run(
                ["bash", str(project_bootstrap), ".infra/bin/align-ci.sh", "--dry-run"],
                env=env,
                text=True,
                capture_output=True,
                timeout=3,
            )

            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(
                call_log.read_text().splitlines(),
                [
                    "latest",
                    "--project",
                    str(project),
                    "--tool",
                    "rs-infra-style",
                    "--",
                    "fix",
                    "--dry-run",
                ],
            )


if __name__ == "__main__":
    unittest.main()
