import importlib.util
import tempfile
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    'gpu_sysfs', Path(__file__).resolve().parents[1] / 'stage-gpu-sysfs.py')
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class GpuSysfsTests(unittest.TestCase):
    def test_snapshot_resolves_discovery_paths_and_excludes_controls(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / 'sys'
            gpu = source / 'devices/pci0000:00/0000:00:02.0'
            gpu.mkdir(parents=True)
            (source / 'bus/pci').mkdir(parents=True)
            (source / 'class/drm').mkdir(parents=True)
            (gpu / 'subsystem').symlink_to(source / 'bus/pci')
            for attribute in ('vendor', 'device', 'subsystem_vendor',
                              'subsystem_device', 'revision', 'class', 'uevent'):
                (gpu / attribute).write_text('metadata\n')
            (gpu / 'remove').write_text('dangerous control')
            for name, number in [('renderD128', '226:128'), ('card1', '226:1')]:
                node = gpu / 'drm' / name
                node.mkdir(parents=True)
                (node / 'device').symlink_to('../..')
                (node / 'dev').write_text(number + '\n')
                (node / 'uevent').write_text('DEVNAME=dri/' + name)
                (source / 'class/drm' / name).symlink_to(node)
            target = root / 'snapshot'
            MODULE.stage(source, target, ['renderD128', 'card1'])
            discovered = target / 'dev/char/226:128/device'
            self.assertEqual((discovered / 'vendor').read_text(), 'metadata\n')
            self.assertTrue((discovered / 'drm/card1/dev').is_file())
            self.assertFalse((discovered / 'remove').exists())
            for link in target.rglob('*'):
                if link.is_symlink():
                    link.resolve(strict=True).relative_to(target)
            with self.assertRaises(ValueError):
                MODULE.stage(source, root / 'bad', ['../escape'])
            self.assertFalse((root / 'bad').exists())


if __name__ == '__main__':
    unittest.main()
