# Isolated Android shell preflight

Verified on 2026-11-02 with the selected Android 13 x86_64 system image.
The guest shell executed successfully. Android init was not started.

## Run

From the repository root, in a normal terminal, without sudo:

```sh
python3 scripts/probe-shell.py os_images/selected/system.img
```

Developer tools required: Python 3.11 or newer, `debugfs`, and `bwrap`
(Bubblewrap). All were already installed on the test host. These tools are
not added as dependencies of the Droidpc Rust CLI.

The script pins the system image SHA-256 recorded in `IMAGE-BOOT.md`. It
rejects a different image before extracting or executing guest files. Hashing
streams the file; it does not load the full 1.8 GB image into RAM. This is a
development check, not work to repeat for every app launch.

## What runs

The probe extracts exactly four guest files into a temporary directory:

- `/system/bin/sh`
- `/system/apex/com.android.runtime/bin/linker64`
- `/system/apex/com.android.runtime/lib64/bionic/libc.so`
- `/system/apex/com.android.runtime/lib64/bionic/libdl.so`

It supplies the corresponding `/apex/com.android.runtime` and
`/system/bin/linker64` symlinks, then runs one fixed shell self-check.
The extracted binaries total about 3.1 MB; this is file size, not measured
process memory. No vendor image, full rootfs extraction or disk-image mount
is needed for this narrow check.

The image stays open read-only. `debugfs` receives that same file descriptor
and uses fixed extraction paths. Its success status alone is insufficient:
the script also checks that each output exists and has an ELF signature.

The installed Bubblewrap tool creates user, PID, network, IPC, UTS, cgroup
and mount isolation. The probe exposes only the temporary guest root
read-only, a private read-only procfs, and Bubblewrap's minimal `/dev`.
It drops capabilities, clears environment variables and uses a new session.
No host home, host sysfs, GPU, Binder, network interface or writable image
is passed in. Bubblewrap is a development harness here, not a decision to
use it as Droidpc's production container backend.

The shell check verifies that the root cannot be written and that effective
capabilities are zero and `NoNewPrivs` is enabled. A second invocation exits
with code 7, checking that guest failure reaches the caller.

## Observed results

```text
PASS verified system image hash
PASS Android shell, read-only root, no host home/sys/vendor
PASS zero effective capabilities and no-new-privileges
DROIDPC_SHELL_OK
PASS guest failure exit status propagated
PASS temporary root removed
```

The initial attempt used `printf`, which this image's shell expected as an
external command. The fixed probe uses built-in `echo`, keeping the extracted
file set small. The failed attempt's temporary files were also removed.
A host-side follow-up found no remaining `droidpc-shell-*` temporary roots.

The loader warns that `/linkerconfig/ld.config.txt` is absent. This minimal
probe explicitly sets `LD_LIBRARY_PATH` to the extracted Bionic directory.
The warning is printed, not suppressed. This does not validate Android's
full linker namespaces or replace linkerconfig/APEX activation during boot.

## Cleanup and failure handling

Each extraction or guest invocation has a 10-second subprocess timeout.
TemporaryDirectory removes the private files on ordinary failure, timeout
or normal completion. Bubblewrap's die-with-parent option ties guest lifetime
to the harness. Namespace mounts are not made persistent on the host.

Unit tests cover refusing root, rejecting an unverified image before
extraction, and removing temporary files after a simulated extraction error
or guest timeout. The simulated timeout checks Python cleanup, not a live
kernel timeout. The actual guest success and exit-7 paths were exercised.

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover \
  -s scripts/tests -v
```

A SIGKILL or power loss can leave extracted files under `/tmp`; this probe
is not production crash-recovery logic. It is also not a hardened sandbox
for arbitrary untrusted applications. Keep the fixed image hash and command.

## Next gate

Implement and test full read-only system/vendor staging and mount cleanup
before running Android init. This shell preflight validates the executable,
loader and small filesystem view only. It does not validate image mounts,
guest cgroups, Binder, SELinux integration, Android boot, graphics or app
resource use. Host delegation is separately tested in `CGROUP-PROBE.md`.

No RAM or CPU budget for Android is inferred from this result. Preserve the
existing image's app compatibility until measurements justify a change.

References:

- [Bubblewrap](https://github.com/containers/bubblewrap)
- [debugfs read-only mode](https://man7.org/linux/man-pages/man8/debugfs.8.html)
