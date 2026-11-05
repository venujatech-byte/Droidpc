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

The current agent environment has no `/dev/loop-control` or `/dev/dri`, no
effective capabilities, and `NoNewPrivs=1`. There is no connected host shell
tool. Local compilation and unit tests cannot prove this privileged gate.

Check whether the matching card node and vendor allocator let SurfaceFlinger
stay up and whether the stub property is effective. If SurfaceFlinger remains
stable, use the ANR capture to identify the next framework blocker. Then
implement and verify the confined SELinux transition and supervised lifecycle
with persistent `/data`; the permissive diagnostic policy is not runtime
policy. No existing Android guest state or host policy may be modified.
