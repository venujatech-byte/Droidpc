"""Check network helper security context selection without root or host changes."""
from pathlib import Path
import os
import subprocess
import tempfile
import unittest


class NetworkLaunchTests(unittest.TestCase):
    def test_helper_uses_guest_context_only_with_temporary_policy(self):
        source = (Path(__file__).resolve().parents[1] / 'probe-android-init.sh').read_text()
        start = source.index('        set -- /usr/bin/pasta --foreground')
        end = source.index('        network_pid=$!', start) + len('        network_pid=$!')
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            pasta, runcon = root / 'pasta', root / 'runcon'
            pasta.write_text('#!/bin/sh\nprintf "helper:%s\\n" "$@"\n')
            runcon.write_text('#!/bin/sh\nprintf "context:%s\\n" "$1" "$2"\nshift 3\nexec "$@"\n')
            pasta.chmod(0o700)
            runcon.chmod(0o700)
            launch = source[start:end].replace('/usr/bin/pasta', str(pasta)).replace('/usr/bin/runcon', str(runcon))
            for policy in ('0', '1'):
                with self.subTest(policy=policy):
                    subprocess.run(['sh', '-eu', '-c', launch + '\nwait "$network_pid"'],
                                   env=os.environ | {'tmp': directory, 'host_dns': '127.0.0.53',
                                                     'DROIDPC_PROBE_SELINUX_DOMAIN': policy}, check=True)
                    output = (root / 'network.log').read_text()
                    self.assertEqual('context:droidpc_probe_t' in output, policy == '1')
                    self.assertIn('helper:0:0', output)
                    self.assertIn(f'helper:{root}/network.ns', output)
                    self.assertIn('helper:--config-net', output)
