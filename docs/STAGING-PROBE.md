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
[`init.rc`](https://android.googlesource.com/platform/system/core/%2B/refs/heads/android13-release/rootdir/init.rc),
and [`variableloader.cc`](https://android.googlesource.com/platform/system/linkerconfig/%2B/refs/heads/android-s-v2-beta-3/generator/variableloader.cc).
This failure identifies a missing probe prerequisite, not an image or APEX
incompatibility. The final config is still unverified in a valid Android
property environment.

A follow-up read-only `getprop ro.vndk.version` check inside Bubblewrap emitted
the expected missing-linkerconfig warnings and no property value. This
confirms the property area is absent in this probe. Do not work around it by
hard-coding the property into the host environment: Android properties use a
guest shared property area, and that would not validate normal boot behavior.

The matching Android 13 init source confirms `PropertyInit()` creates the
guest property area and loads partition boot properties before init parses
its scripts. Init supports selecting a custom boot script through
`ro.boot.init_rc`, which could avoid importing the stock rc files. However,
second-stage init also performs built-in setup before running those scripts,
including cgroup setup and an early-init trigger. This is a possible route
for a minimal probe, not yet a safe runnable command. See Android 13
[`property_service.cpp`](https://android.googlesource.com/platform/system/core/%2B/refs/tags/android-13.0.0_r27/init/property_service.cpp),
[`init.cpp`](https://android.googlesource.com/platform/system/core/%2B/refs/heads/android13-release/init/init.cpp),
and [`init/README.md`](https://android.googlesource.com/platform/system/core/%2B/refs/heads/android13-release/init/README.md).

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
as-is. Next, design a disposable Android 13 second-stage init probe using a
minimal boot script. First verify Bubblewrap can replace `/proc/sys` and
`/sys` with private tmpfs mounts; this mount-only check starts no Android
process. Then provide the private `kptr_restrict` file and keep cgroups
isolated before considering init. Do not run init until host-global effects
are blocked.

The mount-only check passed on the developer host: `/proc/sys` and `/sys`
both appeared as separate `tmpfs` mounts inside Bubblewrap. This verifies
those paths are hidden for that namespace; it does not verify cgroup
isolation or Android init behavior.
Inspect host AVCs and resolve the `unlabeled_t` contexts; do not weaken host
security or relabel source images. Only after those checks should Droidpc
implement mounts and start init in Rust.

Read-only inspection of the selected image's generic `init.rc` found writes
to `/proc/sys/kernel/sysrq` and `/proc/sys/kernel/modprobe` during
`early-init`, plus scheduler and network sysctl writes later. Do not launch
the stock init sequence in the current probe: a mount and PID namespace do
not guarantee isolation of host-global kernel tunables. Any future init probe
must prevent those writes from reaching the host.

The Android 13 second-stage init also runs `SetKptrRestrictAction` before
`early-init`. It writes `/proc/sys/kernel/kptr_restrict` and aborts if the
value cannot be set in the range 2 through 4. An empty private `/proc/sys`
would therefore stop init before a custom script could run. A property probe
must provide this path as private writable state, while keeping the host's
real sysctl tree hidden. Its cgroup setup also runs before `early-init` and
must not be allowed to write into the host cgroup tree. See Android 13
[`security.cpp`](https://android.googlesource.com/platform/system/core/%2B/95cc6b6f013bc52818dd7eda89632f4fdf21285f/init/security.cpp)
and [`init.cpp`](https://android.googlesource.com/platform/system/core/%2B/refs/heads/android13-release/init/init.cpp).

Android init and the container are still not running.
