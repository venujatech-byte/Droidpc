# Droidpc image inspection and boot requirements

Inspected on 2026-11-02. These are observations and a proposed boot contract,
not a tested container configuration. Android has not been started.

## Original host implementation and third-party guest

Write Droidpc's Rust host runtime independently from Linux interfaces and
upstream specifications. Do not copy or rebrand Android guest runtime code or
scripts. Do not claim the selected guest images were created by Droidpc.

The supplied images contain Android guest-built LineageOS and Android guest guest HALs.
Retain their names, embedded attribution and notices. Keep the images outside
Git. The Droidpc repository license does not replace third-party licenses.
This inspection is not a redistribution or licensing audit.

## Selected local pair

Both paths are relative to the repository root:

- System directory:
  `os_images/lineage-20.0-20260927-VANILLA-x86_64-system/`
- Vendor directory:
  `os_images/lineage-20.0-20260927-MAINLINE-x86_64-vendor/`

Each contains its corresponding `.img` file. The ignored directory
`os_images/selected/` contains relative symlinks to these two files. Images
were not moved, modified or duplicated. Run:

```sh
cargo run -p droidpc-cli -- doctor --images os_images/selected
```

The supplied `lineage-23.2-20261003-nightly-ohm-signed` package is excluded.
Its OTA metadata says `pre-device=ohm` and contains an A/B `payload.bin`, not
this container's system/vendor pair. Do not combine it with these images.

## Provenance checks

The original ZIP files were found in the user's Downloads directory.
Their SHA-256 hashes match the respective `id` fields of the release entries
in the upstream OTA manifests. The streamed, decompressed image hashes also
match the local images under `os_images/`. This verifies correspondence to
the fetched release metadata, not the security or bootability of the guest.

[System manifest][system] and [vendor manifest][vendor] were checked for the
20260927 release. Recorded archive hashes:

System ZIP, 847449341 bytes:

```text
053552725bf4ae25e2d3f7e6f1947203294aa9d73dc17fe1f3abe646e5d3bd8a
```

Vendor ZIP, 189818687 bytes:

```text
d911b8353f6c807b94790b1c41c67e863a9f3dcd1cf3ec0232351398235afd7a
```

Local system image, 1827717120 bytes:

```text
30c87ee44412fee4f50173716a6bb7c6941d3021853f0adab4a249da8a9455ab
```

Local vendor image, 562388992 bytes:

```text
5cc4267517ded28f1cf60a5d2b6e8b88b15d8520de1f55a39de7764e7929a877
```

Doctor itself only checks image signatures. Its image-trust warning remains
accurate for that command; it does not read or enforce this inspection record.

## Observed guest identity and layout

Read with `debugfs` in its default read-only mode, without mounting or
executing anything from either image:

- System `/system/build.prop` identifies LineageOS 20.0, Android 13, API 33,
  `android_guest_x86_64`, and x86_64/x86 ABIs. Native bridge is disabled.
- Vendor `/build.prop` identifies the same device, Android version and ABIs,
  with VNDK 33 and `zygote64_32`.
- Both identify 20260927 builds. This is a candidate matched pair; full VINTF
  compatibility and boot are untested.
- System security patch property is `2026-02-01`; a later build date does not
  imply a matching patch date. Both builds identify as `userdebug/test-keys`.
- Both are raw ext-family filesystems with extents, not Android sparse images.
- The system image includes the root directory tree and `/system`.
  `/init` is a symlink to `/system/bin/init`.
- The vendor image root contains `bin`, `etc`, `lib` and `lib64`. Its intended
  guest mount point is `/vendor`, not the guest root or an overlay lowerdir
  merged with the system image.
- `/data` is a separate mount point for persistent writable Android data.

## Guest requirements that affect our own runtime

These are paraphrased findings from the supplied configuration, not copied
runtime scripts or an implementation recipe.

| Evidence in guest | Consequence for Droidpc |
| --- | --- |
| Root tree and `/init` symlink | Preserve system-as-root layout. |
| Init filesystem stage uses a dummy mount operation | Host setup must provide required mounts. |
| Init service overrides use an account named `host` | Resolve guest UID/GID mapping before launching services. |
| Graphics and audio use guest-specific HALs | Inspect their interfaces; do not invent socket protocols. |
| Composer restart restarts SurfaceFlinger | Boot/logging must handle dependent service failures. |
| Cgroup config lists v1 controllers plus a v2 freezer | Resolve task profiles for Fedora's v2-only hierarchy. |
| Init touches host-sensitive sysfs/debugfs paths | Expose a restricted view; never hand over writable host trees. |
| Vendor manifest declares task/window/display services | Presence alone does not establish usable desktop integration. |

The vendor image contains a Android guest composer, GBM/minigbm allocators and
Intel/Radeon/virtio Vulkan libraries. Their presence does not establish
which path works on the host GPU. No graphics selection has been made yet.

## Proposed first boot contract

1. Use a private mount namespace with private mount propagation. Keep image
   files read-only. Add only the writable overlays required by the guest.
2. Mount the system root, mount vendor at `/vendor`, and provide a separate
   persistent `/data`. Record every owned resource for cleanup.
3. Provide isolated PID, IPC, UTS and network namespaces, private `/proc`,
   controlled `/dev`, and a restricted `/sys` view. Decide user-ID mappings
   explicitly; user-namespace availability alone is insufficient.
4. Allocate private Binder devices. Do not reuse another running Android
   instance's Binder devices or delete resources owned by that instance.
5. Establish delegated cgroup limits and guest-compatible task profiles.
   Do not switch the host to cgroup v1 or disable host security.
6. Resolve init entry mode, boot properties, guest `host` account, APEX setup,
   SELinux interactions and required capabilities before invoking init.
7. First prove a shell-only boot with logs, stop, failed-start rollback and
   restart recovery. Add a network interface/DNS and graphics only as needed.

This contract is not ready to execute. The next task is resolving the cgroup
and init/property requirements into a minimal, reviewable setup for the
selected image. No container crate or privileged boot code was added here.

## Validation performed

- Ignore rules cover all of `os_images/`; no images are tracked by Git.
- Local SHA-256 hashes match streamed image contents in the original ZIPs.
- Archive hashes match the fetched upstream release IDs.
- Doctor outside the development sandbox reports 0 problems and 6 warnings
  for `os_images/selected`, including successful GPU device access.
- Workspace format, compile, Clippy and all eight tests pass.

[system]: https://ota.example.invalid/system/lineage/android_guest_x86_64/VANILLA.json
[vendor]: https://ota.example.invalid/vendor/android_guest_x86_64/MAINLINE.json
