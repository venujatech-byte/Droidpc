# Android container implementation progress

## Current state

- Latest host-capability probe reached `sys.boot_completed=1` before the
  outer 180-second timeout. SurfaceFlinger, zygote, the graphics allocator and
  HWC were running; ActivityManager and PackageManager checks succeeded. The
  `INIT_PROBE_STATUS=124` is the probe's outer timeout, not evidence that
  Android failed to boot. Earlier runs include one incomplete boot, so repeat
  reliability is still unproven.
- Android `system` (UID 1000), Launcher (10102) and SystemUI (10108) all opened
  both `/dev/dri/card1` and `/dev/dri/renderD128`; both nodes were mode 0666 in
  this private probe. A screenshot from the subsequent run shows the Android
  lock screen, status bar and navigation controls, confirming visible frame
  rendering. This is a rendering milestone; repeatability and production
  runtime isolation are still unverified.
- The same run repeatedly failed to start `incidentd` because init could not
  apply its requested capabilities (`cap_set_proc(524320): Operation not
  permitted`). Guest UID cgroup placement is also still unresolved: the guest
  sees `cpu io memory pids`, has none enabled for children, and remains at
  `0::/`. Host SELinux AVCs for the probe domain were marked `permissive=1`.
  This run therefore does not validate an enforcing production policy or
  delegated cgroup setup.

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
- Android init has now reached `sys.boot_completed=1` in more than one
  supplied run, including the latest probe. Repeat reliability and a
  production start/stop lifecycle remain unverified.
- Earlier runs showed EGL `EACCES` and SurfaceFlinger buffer failures. The
  latest run's Android system, launcher and SystemUI users all opened the
  private mode-0666 DRM nodes, and Android boot completed. Actual EGL frame
  presentation still needs direct confirmation. The probe maps `renderD128`
  to its actual primary card (`card1`) and keeps both nodes private to the
  guest mount namespace.
- The vendor allocator init rule only starts
  `vendor.graphics.allocator-4-0` when `ro.hardware.gralloc=minigbm_gbm_mesa`.
  The probe used `gbm`, so it now sets the exact image value and reports
  allocator/composer service state in the final snapshot.
- The probe supplies the vendor image's stub sensor property and host
  user/UID/GID properties used by vendor audio and HWC
  services running as `host`. It reports the guest UID/GID maps and audio
  service state alongside HWC. SystemServer ANR traces are reported by path
  to keep diagnostics bounded.
- The DRM probe now opens the staged nodes as Android `system` and as the
  launcher (UID 10102) and SystemUI (UID 10108), matching the app identities
  seen in the captured crash log. The latest host run passed all six opens.
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

The installed Fedora container policy source adds useful host-specific
evidence: it labels the guest rootfs, transitions guest init into
`container_runtime_t`, and allows that domain's private Binder context manager.
The failed Droidpc probe stayed in `unconfined_t`, which explains its Binder
denial. This is a design lead, not Droidpc policy: its own rootfs label and
confined transition are still required, and the host policy must not be a
runtime dependency.

## Next action

The selected image files currently pass the Rust verifier:

```sh
cargo run -p droidpc-container --example verify -- os_images/selected
```

The check completed successfully in the current workspace with
`Verified system and vendor; no mounts created`. The earlier host staging
attempt reported a hash mismatch, so rerun the privileged stage example before
depending on kernel mounts. It uses a new private
`/tmp/droidpc-stage-<pid>-<attempt>` directory and does not start Android or touch
persistent guest data. It attempts normal cleanup and rolls back the system
mount if vendor setup fails; injected failures after successful mounts and concurrent loop
allocation still need runtime coverage.

The next implementation milestone toward a live window is a supervised,
persistent Android session with bounded stop/recovery. An experimental
`DROIDPC_PROBE_WINDOW=1` mode streams Android H.264 screen recording through
GStreamer into a GTK window, forwards taps/swipes/basic navigation keys through
a validated guest input helper, and stops the guest on window close or Ctrl-C.
Guest screenrecord support and required GStreamer/GTK libraries are present; the
privileged guest/window/input run remains unverified. Window mode preserves
guest data in `~/.local/share/droidpc-probe/data`; it still uses diagnostic
capabilities and remains a development bridge rather than the production
runtime.

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
required. A later check inside this Codex execution sandbox confirmed that
`sudo -n` is also blocked by `NoNewPrivs=1`; this sandbox result does not test
the user's host terminal. Privileged staging and Android boot remain
unverified.

The current workspace was rechecked with `cargo test --workspace --all-targets`
(16 tests), `cargo clippy --workspace --all-targets -- -D warnings`, and
`python3 -m unittest discover -s scripts/tests` (6 tests); all passed. The
`stage` and `binder` examples were rebuilt after their source files, and the
rebuilt `verify` example checked both selected image hashes successfully.
These checks do not validate privileged mounts or Android runtime behavior.

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

The latest host-capability run set `sys.boot_completed=1` and passed framework
service checks before the outer timeout. The probe remains diagnostic only;
production boot requires a scoped capability, enforcing SELinux and delegated
cgroup contract. Visible EGL output is now confirmed; reliable repeated boots
and graceful start/stop remain open.
