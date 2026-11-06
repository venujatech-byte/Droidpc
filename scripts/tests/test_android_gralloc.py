"""Both buffer backends retain Mesa hardware rendering and select matching properties."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


class GrallocTests(unittest.TestCase):
    def test_backend_properties_keep_gpu_rendering(self):
        source = (Path(__file__).resolve().parents[1] / 'probe-android-init.sh').read_text()
        start = source.index('if [ -n "$render_node" ]; then\n    printf')
        end = source.index("printf '%s\\n' 'androidboot.mode=normal'", start)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'system-build.prop'
            for backend in ('gbm', 'minigbm_gbm_mesa'):
                with self.subTest(backend=backend):
                    path.write_text('')
                    subprocess.run(['sh', '-eu', '-c', source[start:end]], env=os.environ | {
                        'tmp': directory, 'render_node': '/dev/dri/renderD128', 'gralloc_backend': backend,
                    }, check=True)
                    properties = dict(line.split('=', 1) for line in path.read_text().splitlines())
                    self.assertEqual(properties['ro.hardware.egl'], 'mesa')
                    self.assertEqual(properties['ro.hardware.gralloc'], backend)
                    self.assertEqual('gralloc.gbm.device' in properties, backend == 'gbm')
                    self.assertEqual(properties['gralloc.gbm.legacy'], 'true' if backend == 'gbm' else 'false')
