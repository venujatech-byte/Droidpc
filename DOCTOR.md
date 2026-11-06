# Droidpc doctor

The initial read-only prerequisite report is implemented. It does not boot
Android or certify that every Android application will work.

Run from a terminal in your normal Wayland desktop session, without sudo:

```sh
cargo run -p droidpc-cli -- doctor
```

The default image directory is `/var/lib/droidpc/images`. To inspect an
existing pair elsewhere:

```sh
cargo run -p droidpc-cli -- doctor \
  --images /var/lib/droidpc/images
```

The selected directory must contain `system.img` and `vendor.img`. Droidpc
only reads these files and does not start a container.
Paths containing spaces must be quoted.

## What the report checks

- Build target, running kernel, total and available memory.
- Registered BinderFS, OverlayFS and ext4 support, plus loop-device visibility.
- Visible mount, PID, UTS, IPC, network, user and cgroup namespace handles.
- Cgroup v2 mount discovery, visible-root and current-cgroup CPU, memory and
  process-count controllers, and controllers enabled for children.
- Wayland socket presence, including an absolute `WAYLAND_DISPLAY` path.
- GPU render nodes and permission to open them for reading and writing.
  Device handles close immediately; no GPU commands are submitted.
- Visible SELinux and AppArmor state without changing policy.
- Image readability, size and initial ext-family or Android sparse signature.
  At most 1082 bytes are read from each image, not the entire image.

Cgroup mount roots and escaped paths are handled. Multiple visible cgroup v2
mounts, inaccessible files, and unsupported layouts are reported as unknown
rather than guessed. A missing registered filesystem may simply need its
kernel module loaded; doctor does not load modules.

## Results and exit codes

- `PASS`: the specific check succeeded, not a complete compatibility verdict.
- `INFO`: observed context, without a pass/fail judgment.
- `WARN`: follow-up needed or an intentionally untested runtime capability.
- `FAIL`: a definite mismatch for the initial x86_64 Linux target.
- `UNKNOWN`: the check could not establish the result in this environment.

Exit status `0` means no `FAIL` or `UNKNOWN` results. Warnings can remain.
Exit status `1` means at least one failure or unknown result.
Exit status `2` means invalid command-line arguments.

Checks continue after individual failures, and the report ends with totals.
Missing images are reported individually. No report is a promise that Android
can boot or that resource-limit permissions have been granted.

## Limits of a read-only report

Run outside development sandboxes to inspect the normal host environment.
Sandboxes can hide GPU devices or expose cgroup filesystems read-only. A
report from such an environment describes that environment, not the full PC.
A successful device open is only a snapshot of access at that moment.

The report does not create namespaces, mount images, allocate Binder devices,
create cgroups, test delegation, or apply resource limits. It also does not
connect to Wayland, negotiate graphics protocols or test guest GPU drivers.
Inherited `WAYLAND_SOCKET` file descriptors are reported as untested.

Image signatures do not validate filesystem integrity, image architecture,
version pairing, HAL configuration or provenance. Unknown image formats need
inspection; sparse images need conversion before direct mounting. Checksum
verification requires a trusted manifest for the chosen release and remains
part of image provisioning. No boot compatibility is inferred from a magic
number. Memory minimums and free-space budgets await a measured runtime.

Keep SELinux/AppArmor enabled. Actual container policy and privileged resource
setup will be implemented and tested with the container lifecycle.

## Validation

```sh
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Tests cover exact names, mount paths and escapes, cgroup membership, Wayland
paths, image signatures, argument handling and continuing after image errors.

Implementation references:

- [Cgroup v2](https://docs.kernel.org/admin-guide/cgroup-v2.html)
- [Mount information](https://man7.org/linux/man-pages/man5/proc_pid_mountinfo.5.html)
- [BinderFS](https://docs.kernel.org/admin-guide/binderfs.html)
- [Wayland client API](https://wayland.freedesktop.org/docs/html/apb.html)
- [ext superblock](https://docs.kernel.org/filesystems/ext4/super.html)
