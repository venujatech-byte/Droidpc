# Android container implementation progress

## Current state

- Image verification exists in `droidpc-container`.
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
  probe passed `/dev/dri/renderD128` but not its matching primary DRM card, so
  it now identifies the card through sysfs and passes it when present. The
  current host inventory maps this render node to `card1`, not `card0`. This
  change is not runtime-verified.
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

Check whether the card node lets SurfaceFlinger stay up and whether the stub
property is effective. If SurfaceFlinger remains stable, use the ANR capture
to identify the next framework blocker. Then implement and verify the confined
SELinux transition and supervised lifecycle with persistent `/data`; the
permissive diagnostic policy is not runtime policy. No existing Android guest state
or host policy may be modified.
