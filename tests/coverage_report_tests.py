#!/usr/bin/env python3
"""Tests for coverage report package selection."""

import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent


class CoverageReportTests(unittest.TestCase):
    def test_package_scope_passes_selected_package_to_report(self):
        with tempfile.TemporaryDirectory() as temp:
            project = Path(temp)
            (project / ".infra/coverage").mkdir(parents=True)
            (project / ".infra/coverage/coverage.json").write_text('{"scope":"package"}', encoding="utf-8")
            (project / "Cargo.toml").write_text("", encoding="utf-8")
            (project / "bin").mkdir()
            cargo = project / "bin/cargo"
            cargo.write_text("""#!/usr/bin/env python3
import json, os, pathlib, sys
args = sys.argv[1:]
with open(os.environ['TEST_CARGO_LOG'], 'a', encoding='utf-8') as log:
    log.write(' '.join(args) + '\\n')
if args[0] == 'metadata':
    print(json.dumps({'packages': [{'name': 'sample', 'manifest_path': str(pathlib.Path(os.environ['RS_INFRA_PROJECT_ROOT']) / 'Cargo.toml')}]}))
elif '--output-path' in args:
    path = pathlib.Path(args[args.index('--output-path') + 1])
    path.parent.mkdir(parents=True, exist_ok=True)
    totals = {key: {'covered': 1, 'count': 1} for key in ('functions', 'lines', 'regions')}
    path.write_text(json.dumps({'data': [{'totals': totals}]}), encoding='utf-8')
elif '--output-dir' in args:
    pathlib.Path(args[args.index('--output-dir') + 1]).mkdir(parents=True, exist_ok=True)
""", encoding="utf-8")
            cargo.chmod(0o755)
            env = os.environ.copy()
            env.update({"RS_INFRA_PROJECT_ROOT": str(project), "RS_INFRA_SHARED_ROOT": str(ROOT),
                        "TEST_CARGO_LOG": str(project / "cargo.log"), "PATH": str(project / "bin") + os.pathsep + env["PATH"]})
            result = subprocess.run(["bash", str(ROOT / ".infra/lib/coverage-report.sh")],
                                    env=env, capture_output=True, text=True, check=False)
            self.assertEqual(result.returncode, 0, result.stderr)
            calls = (project / "cargo.log").read_text(encoding="utf-8").splitlines()
            self.assertTrue(any("llvm-cov report --package sample --json" in call for call in calls))
            self.assertEqual(json.loads((project / "ci-summary.json").read_text())["lines"]["covered"], 1)


if __name__ == "__main__":
    unittest.main()
