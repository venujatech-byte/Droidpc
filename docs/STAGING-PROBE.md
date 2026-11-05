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

**Cgroup namespace check (2026-11-04):** Bubblewrap reported `0::/` inside
the new cgroup namespace. A read-only bind of host `/sys/fs/cgroup` stayed
read-only; remounting it writable failed with `permission denied`, and opening
`cgroup.procs` for writing failed with `EACCES` without writing data. A new
read-write cgroup2 mount over private `/sys` also failed with `permission
denied`. This confirms the cgroup mount boundary for the tested unprivileged
Bubblewrap user namespace only. It does not prove the behavior of a privileged
runtime or an Android init process with different capabilities.

**Private init inputs (2026-11-04):** a shell-only Bubblewrap check replaced
`/proc/cmdline` with a temporary `androidboot.init_rc` entry and exposed
`/proc/sys/kernel/kptr_restrict` as a regular file on private tmpfs. Reading
and writing the value `4` succeeded there. No Android process ran. This
validates the probe plumbing only; second-stage init still needs a bounded
run with the stock init directories masked and system/vendor mounted
read-only.

**Second-stage property probe (2026-11-04):** init started and logged
properties loaded from the selected images, including `ro.product.device`
`android_guest_x86_64` and `ro.product.brand` `android_guest`. It aborted before
processing the custom `early-init` action because private `/dev/socket` did
not exist; property-service socket creation failed. The images remained
read-only and the probe returned before running the `getprop` helper. A retry
created `/dev/socket`; init then started the property service and advanced
to `SetupMountNamespaces`, which failed with `Read-only file system`.

Android 13 `mount_namespace.cpp` creates `/mnt/user` and `/mnt/installer`
during this setup. Since the image root is read-only, expose `/mnt` as a
private tmpfs in Bubblewrap before starting init. This keeps those temporary
mount points out of the image and host. See
[`mount_namespace.cpp`](https://android.googlesource.com/platform/system/core/%2B/832f9f1dbd2fd6f33f27622d7fd51e4a92534b31/init/mount_namespace.cpp).
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

**Property and linkerconfig probe (2026-11-04):** a private `/mnt` tmpfs
allowed `SetupMountNamespaces` to complete. Init loaded
`ro.vndk.version=33`; binding `/dev/__properties__` to a temporary host
directory preserved the property area after init was stopped. A separate
Bubblewrap stage bootstrapped APEX and generated a nonempty
`/linkerconfig/ld.config.txt`; `/system/bin/sh` then ran successfully. The
expected initial linker warnings occur before the bootstrap config exists.
The init stage still logs cgroup setup failures in its private `/sys` and
`/dev` trees. No Android services or graphics were started.

**Host cgroup delegation (2026-11-04):** the existing
`scripts/probe-cgroups.py` passed in a temporary systemd delegated service.
It created and removed an empty child cgroup and read back temporary CPU,
memory, and PID limits. This verifies host delegation only, not the guest's
cgroup configuration.

**Guest ID-map probe (2026-11-04):** the Bubblewrap invocation used by the
init probe reported only `0 0 1` in both `/proc/self/uid_map` and
`gid_map`. The host has `venujasilva:524288:65536` in both `/etc/subuid` and
`/etc/subgid`, and unprivileged `unshare --user --map-auto
--map-root-user` reported mappings `0 1000 1` and `1 524288 65535`. Thus the
host has subordinate ranges, but the init probe did not use them. Its
`lchown()` failures for Android-owned cgroup paths and service `setgroups`
failure remain unresolved. Do not infer that the wider map alone fixes the
`setgroups` restriction; verify both mappings and setgroups behavior in a
disposable namespace before running init again.

The focused `sudo ./scripts/probe-android-userns.sh` check passed directly
inside Bubblewrap and a temporary user namespace: UID 1000, GID 1000 and
`setgroups` all worked. The probe exposed only read-only `/usr` and `/lib64`.
It changed no host mappings or files. The mapped init retry is below.

Android init and the container are still not running.

**Mapped init probe (2026-11-04):** a bounded Bubblewrap run with the verified
UID/GID mapping reached the custom `early-init` action and wrote its marker.
The wrapper timed out with status 124. The log showed `SetupCgroups` rejected
the run because init was not PID 1; it was a child of the probe shell.
The probe now starts init directly as PID 1 in its private PID namespace and
stores the marker in its own temporary `/data`. It does not start the stock
Android services.

**Android image ownership mapping (2026-11-05):** the first full stock-init
run exposed a flaw in the earlier mapping. It mapped guest ID 0 to host root,
then mapped other guest IDs to subordinate host IDs. Image files keep their
numeric Android owners, so a guest service UID such as `logd` (1036) could not
match the image's UID 1036; `/system/bin/logd` is mode `0550`. The boot probe
now identity-maps IDs 0–65535 inside its temporary user namespace so Android
service IDs can access their image files. The next bounded run must confirm
whether this also clears the early BoringSSL failure. Binder still returned
`Permission denied` despite a `0666` binderfs device; SELinux diagnostics are
being captured to determine whether that has a separate cause.

The host-side Binder open succeeded while the same device failed inside the
guest namespace. The probe now mounts Binderfs from Android init after the
private IPC namespace exists, then checks both the device and its Android
`/dev/binder` link. This follows the Android init setup pattern and keeps the
Binder instance inside the guest namespace. See [Android 13 init.rc](https://android.googlesource.com/platform/system/core/%2B/refs/heads/android13-release/rootdir/init.rc)
and the [kernel Binderfs guide](https://android.googlesource.com/kernel/common/%2B/864f2d3482f4b/Documentation/admin-guide/binderfs.rst).

The guest Binder device now opens, but Android service managers cannot claim
the Binder context manager: the host audit log records SELinux denials for
`set_context_mgr` while the host policy is enforcing. The Android processes
run in the host's `unconfined_t` domain because the probe does not load or
replace the host SELinux policy. The BoringSSL diagnostic also found that the
probe's global 64-bit `LD_LIBRARY_PATH` caused the 32-bit self-test to load a
64-bit `libcrypto.so`; the override is removed so Android's generated linker
configuration chooses libraries by ABI. A successful self-test will not by
itself remove the separate host SELinux restriction.

**Latest stock init run (2026-11-05):** the 32-bit BoringSSL self-test now
passes, init reaches `late-init`, both Binder device paths open, and
`INIT_TRIGGER_OK` is present. This is still not a boot: service managers are
denied `set_context_mgr` by the enforcing Fedora policy, and critical `lmkd`
exits four times before boot completion. The guest log also shows init cannot
change ownership or mode on `/proc/pressure/memory`; this may prevent `lmkd`
from configuring its PSI monitor. The run ends with `INIT_PROBE_STATUS=129`
and `ANDROID_BOOT_NOT_COMPLETED`.

The next diagnostic is `sudo ./scripts/probe-android-init-selinux.sh`. It
loads `droidpc_probe_t` as a permissive domain while the probe runs, then
removes that temporary policy module on exit. The Android init process tree
uses the domain so Binder manager calls can proceed without disabling host
SELinux globally. The wrapper also overlays a temporary `lmkd.rc` that runs
`lmkd` as guest root within the existing user namespace. If `lmkd` stays
alive, this isolates its failure to PSI access for a follow-up fix. Neither
override modifies the Android image. This diagnostic run is not a production
security configuration; do not leave the temporary module installed. If the
wrapper is forcibly killed before cleanup, remove it with
`sudo semodule -r droidpc_probe`.

**Temporary SELinux-domain run (2026-11-05):** the process context was
`droidpc_probe_t`, Binder-dependent `vdc` commands completed, and init
advanced through service startup. The 1 GiB `RLIMIT_MEMLOCK` change worked:
`bpfloader` started, but BPF map creation failed with `Operation not
permitted`; `/sys/fs/bpf` was also unavailable in the guest. Init continued
starting services. `zygote` then exited after init could not set its priority,
and `lmkd` eventually exited four times and caused a fatal reboot. Running
`lmkd` as guest root did not prevent its later failures.

Logcat showed that `lmkd` exits because PSI monitor access returns `EACCES`,
and there is no cgroup-v1 `memory.pressure_level` fallback. The latest run
reached the full 45-second timeout; ART started and accepted zygote socket
connections, but Android did not set `sys.boot_completed=1`. Linux restricts
unprivileged PSI monitors to 2-second-multiple windows, whereas Android
`lmkd` uses a 1-second window. Mapping `lmkd` to the PSI file owner would
therefore not solve the monitor failure. The current user namespace cannot
grant the required host-level `CAP_SYS_RESOURCE`; the next step needs a
different runtime/kernel boundary. An opt-in mode is available with
`sudo env DROIDPC_PROBE_HOST_CAPS=1 ./scripts/probe-android-init-selinux.sh`;
it keeps non-user namespaces private and drops host reboot, module-loading,
raw-I/O, time-setting, and ptrace capabilities. It still grants host
`CAP_SYS_ADMIN`, so run it only on a disposable host. This mode has not been
run. The lmkd/zygote overrides and 1 GiB memlock limit affect only the
temporary probe configuration; the host's global memlock limit is unchanged.

**Host-capability run (2026-11-05):** this opt-in mode reached `system_server`
startup and reported no `lmkd` restart. `system_server` then logged a PSI
trigger write failure (`EINVAL`), and SurfaceFlinger crashed with signal 11.
The run still ended at the 45-second probe timeout before
`sys.boot_completed=1`; the shutdown lines are consistent with that timeout.
The next bounded run is extended to 120 seconds and captures the additional
`LowMemDetector`, `SurfaceFlinger`, and crash tags.

The 120-second host-capability run started `system_server` but did not complete
boot. SurfaceFlinger reported that `ro.sf.lcd_density` was missing and then
crashed in `SkiaGLRenderEngine::create`. The probe now adds the conventional
density value (`160`) to a temporary `/system/build.prop` overlay and includes
DEBUG crash frames in its log summary; source images remain unchanged.

The next run no longer logged the missing-density error, confirming the
overlay was read. SurfaceFlinger still crashed: the backtrace ends in the
vendor `/vendor/lib64/egl/libGLESv2_angle.so` at
`egl::Thread::getContext()` after a null pointer dereference. The vendor
`hwcomposer.android_guest.so` service repeatedly exits with status 1. The vendor
image is Android API 33 and includes
`vulkan.pastel.so`; its init property handling maps
`ro.hardware.egl=swiftshader` to ANGLE plus the Pastel software Vulkan driver.
The run confirms those effective properties (`angle`, `default`, `pastel`),
and no SurfaceFlinger SIGSEGV appears in the captured log. The composer still
exits before registering its HIDL service. Its Android guest HWC library connects
to a Wayland compositor. The probe exposes the host socket in the guest and
captures composer, EGL, Vulkan and Wayland errors. One run reported
`HOST_WAYLAND_SOCKET_MISSING`; socket detection now searches
`/run/user/<invoking-uid>/wayland-*` when the requested display socket is
absent. The latest run mounted the socket, but the HWC still could not connect:
its log shows `XDG_RUNTIME_DIR=/run/user/1000` while the socket was mounted
under `/tmp`. The probe now mounts it at the guest's `/run/user/<uid>` path
used by HWC. The following run exited before init because Bubblewrap could not
create `/run/user` in the read-only image. The probe now stages `/run/user`
under its temporary directory and bind-mounts that tree over `/run`. Re-run to
see whether the composer registers. A follow-up attempt exposed that the
Wayland branch had omitted the `/tmp` tmpfs used by probe helpers; that tmpfs is
now present in both branches. Boot remains incomplete until SurfaceFlinger and
the composer stay running.

The latest run confirms the socket path fix: HWC no longer logs a Wayland
connection failure, SurfaceFlinger connects to the composer, and the display
hotplug event is delivered. SurfaceFlinger then aborts with `output buffer not
gpu writeable`. The log also shows SwiftShader rejecting an
`AHardwareBuffer` format and `gralloc.default` failing ashmem allocations.
Android guest sets `sys.use_memfd=true` when `/dev/ashmem`
is absent, but the probe had not done that. It now sets the property at
`early-init` and reports its effective value; rerun to see whether that resolves
the allocation failures and SurfaceFlinger abort. Android boot remains
incomplete. The host's existing Android guest config expects
`/dev/dri/renderD128` with Mesa/GBM, but the Droidpc probe hid host devices
and forced software rendering. The probe now binds that node and
selects Mesa/GBM when it exists, retaining SwiftShader/Pastel as the fallback.

The next run confirms `sys.use_memfd=true`. SystemServer progressed through
`StartDropBoxManager` but repeatedly dumped stacks about every 45 seconds; the
final snapshot showed zygote, SurfaceFlinger and boot animation still running,
with `sys.boot_completed` empty. The shown logcat excerpt does not include the
earlier SurfaceFlinger abort. The probe requests SystemServer's native thread
stacks at 110 seconds and, when an ANR trace exists, prints its SystemServer
section instead of the unrelated first 160 lines. This may identify a blocked
thread in one run; it does not itself prove or fix Android boot.

The first PID 1 run exited with status 129 before `early-init`: init aborted
because the empty private `/dev` lacked `/dev/null`. The probe now uses
Bubblewrap's private device setup to provide standard device nodes.

The private-device retry reached `early-init` as PID 1 and timed out as
planned. The selected image attempted to mount v1 `blkio`, `cpu`, `cpuset`,
`memory` and `schedtune`; every attempt failed. The `SetupCgroups` action
still reported success. The [AOSP setup loop][aosp-cgroup-setup] logs a
warning and continues after each controller failure, so action success
does not establish usable guest cgroups. The probe now copies `/proc/mounts`
into temporary `/data` to show which cgroup mounts actually exist.
The first copy used `/proc/mounts`, which is a symlink; Android init
rejected it. The probe now reads `/proc/1/mounts` directly.

The direct `/proc/1/mounts` read succeeded. Its only guest cgroup mount was
cgroup v2 at `/sys/fs/cgroup`, with read-write mount options. The v1 mount
failures remain. This is a working early-init probe, not a usable guest
cgroup layout or a full Android boot.
The probe now also captures the cgroup v2 controller lists without writing
to the hierarchy. This will show which controllers are available to the
guest before designing a vendor profile override.
Android init's `copy` command rejected both cgroupfs control files as
insecure, so that attempt did not reveal their contents. The next bounded
probe reads them through a guest shell action into temporary `/data`.
The shell action started, but `execv` returned `ENOENT`. The selected image
links `/system/bin/linker64` to the runtime APEX. The custom init script
had skipped the image's linkerconfig and APEX bootstrap actions. The probe
now includes those actions before launching the shell. The retry succeeded:
both bootstrap tools exited with status 0, linkerconfig generated its
configuration, and the guest shell exited with status 0. It reported
`cpu io memory pids` available and no child controllers enabled.

[aosp-cgroup-setup]: https://android.googlesource.com/platform/system/core/%2B/691e0e154ad3d666bcd3fb6d21c563700a31ff2f/libprocessgroup/setup/cgroup_map_write.cpp
