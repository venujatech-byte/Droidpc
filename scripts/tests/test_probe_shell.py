import hashlib
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).resolve().parents[1] / 'probe-shell.py'
SPEC = importlib.util.spec_from_file_location('probe_shell', SCRIPT)
probe_shell = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(probe_shell)


class ShellProbeTests(unittest.TestCase):
    def test_wrong_image_is_rejected_before_extraction(self):
        with tempfile.TemporaryDirectory() as temp:
            image = Path(temp) / 'image'
            image.write_bytes(b'not the selected image')
            with (
                patch.object(probe_shell.os, 'geteuid', return_value=1000),
                patch.object(probe_shell.shutil, 'which', return_value='x'),
                patch.object(probe_shell, 'extract') as extract,
            ):
                with self.assertRaisesRegex(RuntimeError, 'hash differs'):
                    probe_shell.probe(image)
                extract.assert_not_called()

    def test_root_execution_is_rejected(self):
        with patch.object(probe_shell.os, 'geteuid', return_value=0):
            with self.assertRaisesRegex(RuntimeError, 'without sudo'):
                probe_shell.probe(Path('/unused'))

    def test_temporary_root_is_cleaned_after_failure_and_timeout(self):
        errors = [
            subprocess.TimeoutExpired('bwrap', 10),
            RuntimeError('extraction failed'),
        ]
        with tempfile.TemporaryDirectory() as temp:
            image = Path(temp) / 'image'
            image.write_bytes(b'test fixture')
            digest = hashlib.sha256(image.read_bytes()).hexdigest()
            for error in errors:
                with self.subTest(error=error):
                    roots = []

                    def extract(source, root):
                        roots.append(root)
                        (root / 'partial').write_text('test')
                        if isinstance(error, RuntimeError):
                            raise error

                    with (
                        patch.object(probe_shell.os, 'geteuid',
                                     return_value=1000),
                        patch.object(probe_shell.shutil, 'which',
                                     return_value='x'),
                        patch.object(probe_shell, 'SYSTEM_SHA256', digest),
                        patch.object(probe_shell, 'extract', extract),
                        patch.object(probe_shell, 'sandbox', side_effect=error),
                    ):
                        with self.assertRaises(type(error)):
                            probe_shell.probe(image)
                    self.assertEqual(len(roots), 1)
                    self.assertFalse(roots[0].exists())


if __name__ == '__main__':
    unittest.main()
