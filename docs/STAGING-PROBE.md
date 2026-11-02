# Read-only full-image staging probe

Checked 2026-11-02 with the pinned system/vendor pair. This was a disposable
host experiment, not a Droidpc runtime implementation.

## Result

- Mounted `system.img` and `vendor.img` with `fuse2fs -f -o
  ro,norecovery,fakeroot` under temporary directories in `/tmp`.
- A separate Bubblewrap namespace viewed the system image as guest `/`, the
  vendor image at `/vendor`, and the unpacked runtime APEX at
  `/apex/com.android.runtime`.
- The Android shell read `/system/build.prop`, `/vendor/build.prop`, the
  init executable path and the Bionic runtime library.
- The shell verified that writes through `/system` and `/vendor` failed.
- `/system/bin/sh` ran successfully from the fully staged images.
- FUSE daemons stopped, both mounts were unmounted, and temporary directories
  were removed after each attempt.

The working FUSE experiment used `fakeroot`: the guest has root-owned files
that a normal host user cannot read. Bubblewrap mapped UID/GID 0 inside its
user namespace. This is a development technique for examining image paths;
it does not establish the ownership mapping or privilege design for Droidpc.

The runtime loader emitted:

```text
linker: Warning: failed to find generated linker configuration
from "/linkerconfig/ld.config.txt"
```

The shell still ran with an explicit Bionic library path. Full Android init
and services require the correct linker configuration and APEX setup.

## What this confirms

The raw images can be presented as separate read-only system and vendor
filesystem trees. The guest shell runs from that combined view in isolated
namespaces. The layout preserves `/system` and mounts vendor separately. It
also confirms the runtime APEX path needed by the shell's linker.

The host user cannot read some root-only image files directly under normal
FUSE permissions. A production kernel mount owned by a correctly mapped
container process may behave differently. Establish that with the real UID
mapping before relying on file visibility.

An unprivileged `unshare --user --map-root-user --mount` experiment tried
`mount -t ext4 -o loop,ro,norecovery` with the resolved system image and a
temporary mountpoint that existed inside the namespace. The command failed
with `ENOENT` (`No such file or directory`). No mount remained. The immediate
cause was not isolated, so this is not evidence that ext4 or loop mounts are
unsupported by the host. The mount path must be traced or retried with an
authorized host-level mount helper before choosing the production backend.

## What this does not confirm

This test used FUSE, not loop devices or kernel ext4 mounts. It did not test
loop ownership/autoclear, mount crash recovery, SELinux policy, Android init,
Binder allocation, guest cgroups, networking, graphics, app launch, or CPU/RAM
use. Bubblewrap was a temporary inspection harness, not a Droidpc dependency.
The images remain unchanged, and FUSE is not selected as the production path.

## Next gate

Before writing a loop/mount manager, resolve the unexplained mount failure,
choose a safe privilege boundary and prove disposable read-only kernel mounts
of both images with correct cleanup.
If this host's policy does not permit the operation, record the exact denial;
do not weaken SELinux or host security. Then run the full-root shell with the
intended UID mapping and APEX/linkerconfig paths. Only after those checks
should Droidpc implement the mounts in Rust.

Android init and the container are still not running.
