"""Validate the persistent Android input bridge with the installed SDK."""
from pathlib import Path
import os
import shutil
import subprocess
import tempfile
import unittest


class InputBridgeTests(unittest.TestCase):
    def test_validation_accepts_fractional_scroll_and_rejects_unsafe_commands(self):
        sdk = Path(os.environ.get('ANDROID_HOME', str(Path.home() / 'Android/Sdk')))
        jars = sorted(sdk.glob('platforms/*/android.jar'))
        if not jars or not shutil.which('javac') or not shutil.which('java'):
            self.skipTest('Android SDK and Java are not installed')
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            check = root / 'InputCheck.java'
            check.write_text('''public class InputCheck {
                public static void main(String[] args) {
                    for (String line : new String[]{"scroll 12 34 .016 -.031", "tap 12 34", "swipe 1 2 30 40 250", "key 4", "text hello%sworld"})
                        DroidpcInput.command(line);
                    for (String line : new String[]{"scroll 0 0 NaN 1", "scroll 0 0 Infinity 1", "scroll 0 0 5 1", "scroll 0 0 .2", "tap -1 0", "key 4;id", "text hello world", "oops 1"}) {
                        try { DroidpcInput.command(line); throw new AssertionError(line); }
                        catch (IllegalArgumentException expected) { }
                    }
                }
            }''')
            source = Path(__file__).resolve().parents[1] / 'android/DroidpcInput.java'
            subprocess.run(['javac', '--release', '8', '-Xlint:-options', '-cp', str(jars[-1]), '-d', str(root), str(source), str(check)], check=True)
            subprocess.run(['java', '-cp', str(root) + os.pathsep + str(jars[-1]), 'InputCheck'], check=True)
