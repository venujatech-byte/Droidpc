# Android container implementation progress

## Current state

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
  layout is unit tested; kernel mount/allocation/cleanup are not yet tested
  through this implementation.
- Android init probes reached framework startup but did not establish
  `sys.boot_completed=1`. There is still no production start/stop lifecycle.
- The latest supplied run selects Mesa/GBM, but Mesa says it cannot open a DRM
  device. SurfaceFlinger then aborts in `validateOutputBufferUsage` with
  `output buffer not gpu writeable`; it restarts until the probe timeout. The
  probe exposed `/dev/dri/renderD128` without its matching primary DRM card,
  and host node ownership may block Android's `graphics` group. It now matches
  the card through sysfs and creates private tmpfs device nodes owned by
  Android's `graphics` group without changing host node permissions. The
  current host inventory maps the render node to `card1`, not `card0`. This is
  not runtime-verified.
- The vendor allocator init rule only starts
  `vendor.graphics.allocator-4-0` when `ro.hardware.gralloc=minigbm_gbm_mesa`.
  The probe used `gbm`, so it now sets the exact image value and reports
  allocator/composer service state in the final snapshot.
- DRM device nodes now live in a private tmpfs with mode `0660` and Android's
  graphics group for the vendor allocator/HWC. Host node ownership and
  permissions remain unchanged; runtime access is not yet verified.
- The probe also supplies the vendor image's `android_guest.stub_sensors_hal=1`
  property and captures the first SystemServer ANR, but the latest supplied
  run did not include a value for the stub property and produced no ANR trace.
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
`/tmp/droidpc-stage-<pid>` directory and does not start Android or touch
persistent guest data. It attempts normal cleanup and rolls back the system
mount if vendor setup fails; injected failures after successful mounts and concurrent loop
allocation still need runtime coverage.

Sandboxed execution has no `/dev/loop-control` or `/dev/dri`, no effective
capabilities, and `NoNewPrivs=1`. Execution outside the sandbox does reach the
host devices with `NoNewPrivs=0`, but `sudo -n true` reports that a password is
required. Privileged staging and Android boot remain unverified.

The GPU snapshot helper was run directly against host sysfs without root:
`renderD128` reported `226:128`, its matching `card1` reported `226:1`, and
both exposed PCI vendor `0x8086` through the staged discovery links. Every
snapshot symlink resolved inside the temporary snapshot. This validates
snapshot construction on the actual GPU, not Mesa initialization or boot.

The next host boot run should use the current probe, which includes the PCI
GPU sysfs snapshot, private graphics-group device nodes, matched primary card,
and correct vendor allocator property:

```sh
sudo env DROIDPC_PROBE_HOST_CAPS=1 ./scripts/probe-android-init-selinux.sh
```

Keep the `HOST_DRM_CARD` line, `Android graphics user DRM access` section,
effective graphics properties, and final boot/service state. These establish
whether device access succeeds, the allocator starts, and SurfaceFlinger
stays up. An empty `sys.boot_completed` still means boot is unproven. If
SurfaceFlinger remains
stable, use the ANR capture to identify the next framework blocker. Then
implement and verify the confined SELinux transition and supervised lifecycle
with persistent `/data`; the permissive diagnostic policy is not runtime
policy. No existing Android guest state or host policy may be modified.
