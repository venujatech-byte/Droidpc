# Android container implementation progress

## Current state

- Latest host boot evidence is inconsistent across runs: one reached
  `sys.boot_completed=1`; the later run reached framework and
  Launcher/SystemUI startup but timed out without it. The later run still
  logged Mesa `failed to get driver name for fd -1`. Its init log shows
  shutdown after the 180-second timeout. This does not establish reliable
  boot completion or visible rendering. Host node permissions remain
  unchanged.

- Image verification exists in `droidpc-container`.
- `loop_device::LoopDevice` retains the verified image handle and configures
  a read-only loop device with autoclear atomically. `mounts::ImageMount`
  mounts ext4 with `ro,nosuid,nodev,norecovery` in a private mount namespace
  and provides explicit unmount/loop/directory cleanup. ABI and directory
  cleanup tests pass; privileged attachment, mount and cleanup through this
  Rust implementation have not been verified.
- The `stage` example checks both image layouts, requires write attempts to
  fail with `EROFS`, explicitly cleans up both mounts, then verifies the
  original image hashes again. It is a staging check, not Android startup.
- Disposable experiments proved read-only system/vendor staging and an
  isolated Android shell. A privileged experiment also verified kernel
  read-only loop mounts in a private mount namespace.
- `binder::BinderFs` now owns a private BinderFS mount, allocates the guest's
  three Binder devices, and explicitly unmounts on cleanup. Its Rust ABI
  layout is unit tested. The `binder` example now checks each device is a
  character device, opens it read/write, and explicitly cleans up; kernel
  mount/allocation/cleanup through this implementation still need a host run.
- Android init reached `sys.boot_completed=1` in one supplied run; the later
  run timed out before setting it. Reliable boot completion remains
  unverified, and there is still no production start/stop lifecycle.
- Earlier runs showed EGL `EACCES` and SurfaceFlinger buffer failures. The
  current guest nodes are mode `0666`; the latest supplied run no longer shows
  those errors, but Mesa still logs `fd -1` and Android boot remains
  incomplete. The probe maps `renderD128` to its actual primary card (`card1`)
  and keeps both nodes private to the guest mount namespace.
- The vendor allocator init rule only starts
  `vendor.graphics.allocator-4-0` when `ro.hardware.gralloc=minigbm_gbm_mesa`.
  The probe used `gbm`, so it now sets the exact image value and reports
  allocator/composer service state in the final snapshot.
- The probe supplies the vendor image's `android_guest.stub_sensors_hal=1`
  property and host user/UID/GID properties used by vendor audio and HWC
  services running as `host`. It reports the guest UID/GID maps and audio
  service state alongside HWC. SystemServer ANR traces are reported by path
  to keep diagnostics bounded.
- The DRM probe now opens the staged nodes as Android `system` and as the
  launcher (UID 10102) and SystemUI (UID 10108), matching the app identities
  seen in the captured crash log. Those app-UID access checks have not yet been
  verified in a host run.
- Additional graphics prerequisite found by inspecting the selected vendor
  image's `/lib64/libdrm.so`: it contains lookups for
  `/sys/dev/char/%d:%d/device/drm`, `/sys/dev/char/%d:%d/device`, and
  `/sys/bus/pci/devices/...`. The probe currently binds an empty private
  directory as `/sys`, so those paths cannot resolve even if DRM nodes open.
  [Upstream libdrm's Linux node identification and PCI discovery code](https://android.googlesource.com/platform/external/libdrm/+/refs/heads/main/xf86drm.c)
  confirms this dependency. This is a likely additional cause of failed Mesa
  device discovery, not proof of the exact failing call in the supplied run.
  The probe now supplies a private snapshot of selected PCI GPU identification
  files and DRM node metadata, preserving relative discovery links under
  `/sys/dev`, `/sys/devices`, `/sys/bus`, and `/sys/class`. These subtrees are
  bound read-only. The snapshot excludes device controls and unrelated host
  devices. Fixture tests verify link resolution and exclusion of a `remove`
  control; actual Mesa discovery with the snapshot remains unverified.

## Blocking runtime contract

The current Android probe runs with a permissive host SELinux domain. The
enforcing host policy, guest UID/GID mapping, cgroup setup, APEX/linkerconfig
startup, and guest init contract must be resolved before starting Android
under an enforcing production boundary. Do not reuse the permissive probe
policy as the runtime policy.

The installed Fedora Android guest policy source adds useful host-specific evidence:
it mounts the guest rootfs as `android_guest_rootfs_t`, transitions guest init into
`container_runtime_t`, and allows that domain's private Binder context manager.
The failed Droidpc probe stayed in `unconfined_t`, which explains its Binder
denial. This is a design lead, not Droidpc policy: its own rootfs label and
confined transition are still required, and the Android guest policy must not be a
runtime dependency.

## Next action

First validate the new Rust image ownership path on the host:

```sh
cargo build -p droidpc-container --example stage
sudo ./target/debug/examples/stage "$PWD/os_images/selected"
```

Success prints `System/vendor staged read-only, cleaned up, hashes unchanged`.
An error is not a passing staging gate. The example uses a new private
`/tmp/droidpc-stage-<pid>-<attempt>` directory and does not start Android or touch
persistent guest data. It attempts normal cleanup and rolls back the system
mount if vendor setup fails; injected failures after successful mounts and concurrent loop
allocation still need runtime coverage.

After staging passes, run the BinderFS check in a disposable namespace:

```sh
cargo build -p droidpc-container --example binder
sudo ./target/debug/examples/binder
```

The BinderFS example uses `/tmp/droidpc-binder-<pid>-<attempt>` so stale
directories from a reused process ID do not block later runs.

Success prints a `PASS` line for each private device and confirms cleanup. The
example does not replace the host Binder devices or start Android.

Sandboxed execution has no `/dev/loop-control` or `/dev/dri`, no effective
capabilities, and `NoNewPrivs=1`. Execution outside the sandbox does reach the
host devices with `NoNewPrivs=0`, but `sudo -n true` reports that a password is
required. Privileged staging and Android boot remain unverified.

The GPU snapshot helper was run directly against host sysfs without root:
`renderD128` reported `226:128`, its matching `card1` reported `226:1`, and
both exposed PCI vendor `0x8086` through the staged discovery links. Every
snapshot symlink resolved inside the temporary snapshot. This validates
snapshot construction on the actual GPU, not Mesa initialization or boot.

Host libdrm was also exercised inside an unprivileged Bubblewrap sandbox,
with the snapshot bound read-only at `/sys` and the real host device nodes
visible. `drmGetDevices2` returned exactly one GPU. A negative control with
an empty `/sys` returned zero GPUs. This establishes actual host-library
device discovery with the snapshot; the Android vendor library and Mesa
initialization still require the guest boot run.

The selected vendor `/lib64/libdrm.so` was then tested directly with extracted
Android Bionic runtime libraries and the system image's libc++. A temporary
minimal ELF called `drmGetDevices2` inside unprivileged Bubblewrap namespaces
with the snapshot and host DRM nodes visible. It found exactly one GPU,
freed the device list, printed `VENDOR_DRM_ONE_GPU`, and exited zero. The
minimal environment emitted the expected missing-linkerconfig warning.
This verifies Android vendor-library discovery, not EGL/Mesa rendering,
allocator operation, privileged image staging, or Android boot completion.

The latest host-capability run still timed out without `sys.boot_completed`.
The next diagnostic snapshot now reports ActivityManager/package service
checks before the timeout and omits the unbounded SystemServer stack dump.
The probe remains diagnostic only; production boot requires the scoped
capability, SELinux and cgroup contract described above.
