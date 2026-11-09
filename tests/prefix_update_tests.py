from __future__ import annotations

import os
from pathlib import Path
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "assets/project-bootstrap/files/.infra/bin/update-dependencies-by-prefix.sh"


class PrefixUpdateWrapperTests(unittest.TestCase):
    def test_wrapper_passes_arguments_to_project_bootstrap(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            project = root / "project"
            bin_dir = project / ".infra" / "bin"
            bin_dir.mkdir(parents=True)
            wrapper = bin_dir / "update-dependencies-by-prefix.sh"
            wrapper.write_bytes(SCRIPT.read_bytes())
            wrapper.chmod(0o755)
            call_log = root / "bootstrap-args.log"
            bootstrap = project / ".infra" / "bootstrap.sh"
            bootstrap.write_text(
                "#!/usr/bin/env bash\n"
                "set -euo pipefail\n"
                'printf "%s\\n" "$@" > "$BOOTSTRAP_ARGS_LOG"\n'
            )
            bootstrap.chmod(0o755)
            result = subprocess.run(
                ["bash", str(wrapper), "--prefix", "qubit-", "--prefix", "rs-", "--dry-run"],
                env={**os.environ, "BOOTSTRAP_ARGS_LOG": str(call_log)},
                text=True,
                capture_output=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(
                call_log.read_text().splitlines(),
                [
                    ".infra/bin/update-dependencies-by-prefix.sh",
                    "--prefix",
                    "qubit-",
                    "--prefix",
                    "rs-",
                    "--dry-run",
                ],
            )


if __name__ == "__main__":
    unittest.main()
