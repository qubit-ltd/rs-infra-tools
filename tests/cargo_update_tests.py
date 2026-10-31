from __future__ import annotations

import os
from pathlib import Path
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "assets/project-bootstrap/files/.infra/bin/cargo-update.sh"


class CargoUpdateTests(unittest.TestCase):
    def test_scans_fixtures_and_skips_manifests_without_targets(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "project"
            script = root / ".infra/bin/cargo-update.sh"
            nested = root / "crates/nested"
            targetless_fixture = root / "tests/fixtures/sync-version-only"
            valid_fixture = root / "tests/fixtures/valid"
            bin_dir = Path(directory) / "bin"
            for path in (script.parent, nested / "src", targetless_fixture, valid_fixture / "src", bin_dir):
                path.mkdir(parents=True)

            (root / "Cargo.toml").write_text(
                '[package]\nname = "root-crate"\nversion = "0.1.0"\nedition = "2024"\n'
            )
            (root / "src").mkdir()
            (root / "src/lib.rs").write_text("")
            (nested / "Cargo.toml").write_text(
                '[package]\nname = "nested-crate"\nversion = "0.1.0"\nedition = "2024"\n'
            )
            (nested / "src/lib.rs").write_text("")
            (targetless_fixture / "Cargo.toml").write_text(
                '[package]\nname = "sync-version-only"\nversion = "0.1.0"\nedition = "2024"\n'
                '[dependencies]\nnum-bigint = "0.5"\n'
            )
            (valid_fixture / "Cargo.toml").write_text(
                '[package]\nname = "valid-fixture"\nversion = "0.1.0"\nedition = "2024"\n'
            )
            (valid_fixture / "src/lib.rs").write_text("")

            script.write_bytes(SCRIPT.read_bytes())
            script.chmod(0o755)
            cargo = bin_dir / "cargo"
            cargo.write_text('#!/usr/bin/env bash\nprintf "%s\\n" "$*" >> "$CARGO_UPDATE_LOG"\n')
            cargo.chmod(0o755)

            log = Path(directory) / "cargo-update.log"
            env = os.environ.copy()
            env.update(PATH=f"{bin_dir}:{env['PATH']}", CARGO_UPDATE_LOG=str(log))
            result = subprocess.run(["bash", str(script)], env=env, text=True, capture_output=True)

            self.assertEqual(result.returncode, 0, result.stderr)
            calls = log.read_text().splitlines()
            self.assertEqual(len(calls), 3, calls)
            self.assertEqual(calls[0], "update")
            self.assertTrue(any(str(nested / "Cargo.toml") in call for call in calls), calls)
            self.assertTrue(any(str(valid_fixture / "Cargo.toml") in call for call in calls), calls)
            self.assertNotIn(str(targetless_fixture / "Cargo.toml"), "\n".join(calls))
            self.assertIn("Skipping Cargo manifest without targets", result.stdout)


if __name__ == "__main__":
    unittest.main()
