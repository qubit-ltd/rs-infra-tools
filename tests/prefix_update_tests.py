from __future__ import annotations

import os
from pathlib import Path
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "assets/project-bootstrap/files/.infra/bin/update-dependencies-by-prefix.sh"


class PrefixUpdateTests(unittest.TestCase):
    def test_dry_run_updates_nested_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            project = root / "project"
            nested = project / "fuzz"
            bin_dir = root / "bin"
            nested.mkdir(parents=True)
            bin_dir.mkdir()
            (project / "Cargo.toml").write_text(
                '[package]\nname = "root-crate"\nversion = "0.1.0"\nedition = "2021"\n'
                '[lib]\npath = "src.rs"\n'
            )
            (nested / "Cargo.toml").write_text(
                '[package]\nname = "fuzz-crate"\nversion = "0.1.0"\nedition = "2021"\n'
                '[workspace]\n[lib]\npath = "src.rs"\n[dependencies]\nserde = "1.0"\n'
            )
            (project / "src.rs").write_text("")
            (nested / "src.rs").write_text("")
            (bin_dir / "cargo").write_text(
                '#!/usr/bin/env bash\n'
                'if [[ "$1" == add ]]; then\n'
                '  if [[ "${2:-}" == --help ]]; then exit 0; fi\n'
                '  printf "%s\\n" "$*" >> "$CARGO_ADD_LOG"\n'
                'else\n'
                '  exec "$REAL_CARGO" "$@"\n'
                'fi\n'
            )
            (bin_dir / "curl").write_text(
                '#!/usr/bin/env bash\n'
                'printf "{\\"crate\\":{\\"max_stable_version\\":\\"1.2.3\\"}}\\n"\n'
            )
            for command in ("cargo", "curl"):
                (bin_dir / command).chmod(0o755)
            log = root / "cargo-add.log"
            env = os.environ.copy()
            env.update(
                PATH=f"{bin_dir}:{env['PATH']}",
                REAL_CARGO=subprocess.check_output(["which", "cargo"], text=True).strip(),
                CARGO_ADD_LOG=str(log),
            )
            result = subprocess.run(
                ["bash", str(SCRIPT), "--project", str(project), "--prefix", "ser", "--dry-run"],
                env=env,
                text=True,
                capture_output=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertTrue(log.exists(), result.stdout)
            self.assertIn(f"--manifest-path {nested / 'Cargo.toml'}", log.read_text())


if __name__ == "__main__":
    unittest.main()
