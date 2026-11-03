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

An `unshare --user --map-root-user --mount` retry traced the same read-only
loop mount. The image and temporary mountpoint existed, but opening
`/dev/loop-control` and the loop devices returned `EACCES` inside the user
namespace. `mount` then printed the misleading message “No such file or
directory.” No mount remained. This identifies a loop-device permission
boundary in the current namespace setup; it does not show that ext4 or loop
support is missing from the host. A privileged helper or a different safe
mount boundary still needs separate evaluation before choosing the production
backend.

**Privileged kernel-mount retry (2026-11-03):** with sudo, both images
mounted in a private mount namespace using `loop,ro,norecovery`. `findmnt`
reported `ro,relatime,seclabel,norecovery`, and the system/vendor
`build.prop` files were readable. The temporary mounts and loop devices were
confirmed gone after the namespace exited. `mount` warned that the filesystems
did not contain SELinux labels. Read-only `debugfs` inspection found
`security.selinux` xattrs on each image root and the checked build properties
(`rootfs`, `system_file`, and `vendor_file` labels). The warning needs a
host-policy/context investigation; do not relabel or modify these images to
silence it.

## What this does not confirm

This test used FUSE for the full-image shell; the later kernel mount check did
not run Android. It did not test loop cleanup after crashes, SELinux policy,
Android init, Binder allocation, guest cgroups, networking, graphics, app
launch, or CPU/RAM use. Bubblewrap was a temporary inspection harness, not a
Droidpc dependency.
The images remain unchanged, and FUSE is not selected as the production path.

## Next gate

Before writing a loop/mount manager, choose a safe privilege boundary. The
privileged disposable read-only kernel mount and cleanup now pass. Record the
host SELinux mode and inspect effective contexts/AVCs for the mounted guest;
do not weaken host security or relabel source images. Then run the full-root
shell with the intended UID mapping and APEX/linkerconfig paths. Only after
those checks should Droidpc implement the mounts in Rust.

Android init and the container are still not running.
