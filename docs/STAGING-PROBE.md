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

On the Fedora host, `getenforce` reported `Enforcing`. `ls -Z` showed
`system_u:object_r:unlabeled_t:s0` for the mounted roots and checked guest
paths, despite the Android xattrs in the image. Effective host contexts and
policy access remain unresolved; do not weaken host SELinux or relabel the
images.

**Kernel-mounted guest shell (2026-11-03):** Bubblewrap isolated the guest
with user/PID/IPC/UTS/network namespaces. It ran `/system/bin/sh` from the
read-only kernel mounts, with vendor mounted separately and the runtime APEX
exposed under `/apex`.
The shell read both build properties and printed
`DROIDPC_KERNEL_MOUNT_SHELL_OK`. The linker still warned that
`/linkerconfig/ld.config.txt` was missing. A follow-up audit query found no
matching AVCs. Android init was not started.

The guest's generic `/system/etc/init/hw/init.rc` creates bootstrap/default
linkerconfig directories, runs
`/system/bin/bootstrap/linkerconfig --target /linkerconfig/bootstrap`, then
starts `apexd --bootstrap` and updates linker config. Only linkerconfig
generation has been probed; no guest init configuration was copied into
Droidpc.

**Linkerconfig probe (2026-11-03):** the bootstrap linkerconfig binary ran in
the isolated shell with only a temporary `/linkerconfig` writable. It created
a nonempty `ld.config.txt`, and a second shell ran. The first shell still
warned because its config did not exist at startup. Linkerconfig also reported
that `/apex/apex-info-list.xml` was missing. A later isolated `apexd
--bootstrap` run exited successfully and created that file in temporary
`/apex`. Its initial linker warning remains expected. A subsequent run
mounted the bootstrap config and let apexd mount all available APEX modules.
The final linkerconfig process aborted with status 134. Its captured fatal
message was `SANITIZER_DEFAULT_VENDOR is not defined`.

This probe did not start Android init or its property service. The temporary
Bubblewrap `/dev` therefore had no Android property area. The selected vendor
image's `build.prop` does contain `ro.vndk.version=33`; linkerconfig reads
that value through Android system properties, not by parsing `build.prop`
itself. Without it, linkerconfig skips loading vendor VNDK and sanitizer
variables, then fails while generating namespaces. AOSP init normally loads
partition build properties before its `update_linker_config` action. See
[`property_service.cpp`](https://android.googlesource.com/platform/system/core/%2B/refs/heads/android13-release/init/property_service.cpp),
[`init.rc`](https://android.googlesource.com/platform/system/core/%2B/refs/heads/android14-gsi/rootdir/init.rc),
and [`variableloader.cc`](https://android.googlesource.com/platform/system/linkerconfig/%2B/refs/heads/android-s-v2-beta-3/generator/variableloader.cc).
This failure identifies a missing probe prerequisite, not an image or APEX
incompatibility. The final config is still unverified in a valid Android
property environment.

## What this does not confirm

This test used FUSE for the full-image shell; the later kernel mount check did
not run Android. It did not test loop cleanup after crashes, SELinux policy,
Android init, Binder allocation, guest cgroups, networking, graphics, app
launch, or CPU/RAM use. Bubblewrap was a temporary inspection harness, not a
Droidpc dependency.
The images remain unchanged, and FUSE is not selected as the production path.

## Next gate

Before writing a loop/mount manager, choose a safe privilege boundary. The
privileged disposable read-only kernel mount and cleanup now pass. Keep the
base images read-only. Do not repeat the isolated final linkerconfig command
as-is: first design a disposable probe that provides Android's property area
and boot properties without exposing host-global init effects. Inspect host
AVCs and resolve the `unlabeled_t` contexts; do not weaken host security or
relabel source images. Only after those checks should Droidpc implement
mounts and start init in Rust.

Android init and the container are still not running.
