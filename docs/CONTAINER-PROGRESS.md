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
- The latest supplied log stops SystemServer timing at
  `DeviceStateManagerService`. The selected vendor image has no
  `/vendor/etc/devicestate/device_state_configuration.xml`, so its default
  provider has no sensor conditions to query; the missing sensor HAL alone
  does not explain that stall. Separately, the vendor Sensors HAL checks
  `android_guest.stub_sensors_hal`; the host-generated property file sets it when
  the host sensor daemon is absent, but the probe had omitted it. The probe
  now supplies that image-required stub setting and reports its effective
  value. It also polls for boot completion and saves the first SystemServer
  ANR trace before the slower native debugger dump.

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

Use the captured SystemServer ANR stack to identify the synchronous boot
stall, then fix the guest/runtime contract that causes it. Do not add a
vendor-property workaround based only on the missing sensor HAL: the selected
vendor image contains no device-state config. After framework boot completes,
implement and verify the confined SELinux transition and supervised lifecycle
with persistent `/data`; the permissive diagnostic policy is not runtime
policy. No existing Android guest state or host policy may be modified.
