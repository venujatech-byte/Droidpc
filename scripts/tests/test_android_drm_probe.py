"""Exercise the embedded guest DRM check without a GPU or root."""
import subprocess
import tempfile
import unittest
from pathlib import Path


class DrmProbeTests(unittest.TestCase):
    def test_missing_openable_and_unopenable_nodes(self):
        source = (Path(__file__).resolve().parents[1]
                  / 'probe-android-init.sh').read_text()
        helper = source.split(
            'cat > "$tmp/read-drm.sh" <<\'SH\'\n', 1
        )[1].split('\nSH\n', 1)[0]
        self.assertIn('exec - 10102 10102 -- /system/bin/sh /tmp/droidpc-read-drm.sh', source)
        self.assertIn('exec - 10108 10108 -- /system/bin/sh /tmp/droidpc-read-drm.sh', source)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            nodes = root / 'dri'
            nodes.mkdir()
            report = root / 'report'
            script = helper.replace('/dev/dri/*', str(nodes / '*')).replace(
                '/data/droidpc-drm-access', str(report))

            def run():
                subprocess.run(['sh', '-c', script, 'probe', str(report)], check=True,
                               start_new_session=True)
                return report.read_text()

            self.assertIn('DRM_GRAPHICS_NODES_MISSING', run())
            node = nodes / 'renderD128'
            node.symlink_to('/dev/null')
            self.assertIn(f'DRM_GRAPHICS_OPEN_OK={node}', run())
            node.unlink()
            # A new session has no controlling terminal: opening tty must fail.
            node.symlink_to('/dev/tty')
            output = run()
            self.assertIn(f'DRM_GRAPHICS_OPEN_FAILED={node}', output)
            self.assertNotIn('DRM_GRAPHICS_OPEN_OK=', output)
            self.assertNotIn('DRM_GRAPHICS_NODES_MISSING', output)


if __name__ == '__main__':
    unittest.main()
