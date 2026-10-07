"""Run the Debian machine utilities' scratch-only behavioral suite on Linux."""
from pathlib import Path
import os
import subprocess
import sys
import unittest


@unittest.skipUnless(sys.platform == "linux", "Machine utilities target Debian/Linux")
class MachineUtilitiesTests(unittest.TestCase):
    def test_scratch_behavioral_suite(self):
        folder = Path(__file__).resolve().parent / "machine"
        env = {**os.environ, "PYTHONDONTWRITEBYTECODE": "1"}
        result = subprocess.run([sys.executable, "-m", "unittest", "discover", "-v"],
            cwd=folder, env=env, text=True, capture_output=True, timeout=60)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
