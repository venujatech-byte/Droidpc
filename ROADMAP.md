# Droidpc build roadmap

Status: Rust workspace and initial read-only `doctor` report implemented.
See [DOCTOR.md](DOCTOR.md) for usage, checks, exit codes and limitations.
Container lifecycle and Android boot are not implemented yet.

## Goal and working agreement

Build a Rust-managed Android container whose supported apps integrate with
the Linux desktop: individual windows, desktop launchers, keyboard and mouse,
audio, clipboard, notifications, and explicitly shared files.

You write the application code. I explain each small step, provide readable
examples when requested, review your changes, and help debug failures.
Use Ponytail full, KISS, and YAGNI throughout. Aim for code lines of at most
80 characters. Prefer one understandable change per lesson and commit.

The host controller and new bridges can use Rust. Android, ART, existing
HALs, graphics drivers, and possibly guest framework patches remain part
of the system; this is not a plan to rewrite Android in Rust.

Native desktop behavior is an acceptance target for a defined app set.
Identical behavior for every Android app, native CPU execution of ARM apps
on x86, and universal game compatibility cannot be promised.

## Corrections to the original design

Keep `Agents.md` and `architecture.md` as the original proposal. Resolve
these issues before implementing the affected sections:

- Mounting images and executing `/init` is insufficient for a working GUI.
  Select a compatible container image, guest init configuration, graphics
  allocator, Hardware Composer, and host integration together. Start from
  existing working guest components where their licenses permit reuse.
- Binderfs needs device allocation through `BINDER_CTL_ADD`; mounting it
  and adding symlinks alone is insufficient. See [kernel documentation][binder].
- Keep persistent Android `/data` separate from system overlay changes.
  Respect the image's partition layout; stacking system and vendor as
  overlapping lower directories does not create `/system` and `/vendor`.
- `Drop` helps normal cleanup but cannot cover process abort, SIGKILL, or
  power loss. Use explicit shutdown plus recovery of owned resources on
  restart. See the [Rust destructor rules][drop].
- A host Wayland receiver does not create an Android task export API.
  Prove guest task/window integration before designing custom IPC.
  [Android's HWC interface][hwc] describes display composition, not the
  proposed ready-made TaskId socket service.
- Passing DMA-BUF descriptors can avoid CPU pixel copies. It does not
  guarantee zero CPU use, no GPU composition, or a particular frame rate.
  Buffer ownership, release, format/modifier support, and synchronization
  must work across the complete graphics path.
- A PipeWire socket mount alone does not implement an Android audio HAL.
  Binder access alone does not grant Android system-service permissions.
- The Ponytail coding skill and GNOME Ponytail daemon are separate tools.
  Verify the daemon's installed API and compositor support before using it.
  Do not assume the original document's D-Bus methods exist everywhere.
- Rust does not guarantee real-time scheduling or eliminate Android's GC.
  Discard the unmeasured boot-time and translation-speed promises.
- Property strings cannot establish verified boot or official game support.
  [Verified Boot][verified] requires an actual trust chain. Compatibility,
  certification, integrity checks, and publisher acceptance are separate.

## Smallest initial design

Start with `droidpc-cli` and `droidpc-container` only. Add crates when
a working feature needs a separate boundary, following the proposed names.
Keep desktop integration in the user's session. Restrict privileged setup
to container lifecycle and the specific resources it needs.

Use one shared Android instance for the supported apps. Prefer event-driven
work and bounded queues. Reuse the selected image's working graphics path
before considering a replacement. A Rust controller over an existing
container backend is a valid first milestone; replace that backend only
when measurements or a concrete requirement justify the added work.

## Build order and completion gates

### 0. Establish the supported machine and baseline

1. Record distro, kernel, CPU architecture, RAM, GPU and driver, compositor,
   session type, and Rust toolchain.
2. Check binder/binderfs, namespaces, cgroups, overlayfs, render nodes,
   and the host security policy. Do not disable host security to pass.
3. Choose one guest image version and verify its source, checksum, layout,
   required HALs, and compatibility with the selected backend.
4. Choose three initial apps: a simple app, a text-input app, and a graphics
   app. Begin with bytecode-only or matching host-architecture APKs.
5. Measure an existing working Android container on this machine when
   available. Record limitations if a reference baseline is unavailable.

Done: a written host/image/backend combination, app list, baseline, and
explicit blockers. Do not begin privileged container code without this.

### 1. Create the Rust controller

1. Create the workspace and two initial crates, with no unused modules.
2. Implement read-only host diagnostics with clear actionable errors.
3. Introduce lifecycle commands only as their behavior is implemented.

Done: diagnostics report supported and unsupported prerequisites reliably;
formatting, compilation, and relevant checks pass.

### 2. Boot and stop one Android session reliably

1. Integrate the selected backend and compatible image layout.
2. Keep base images read-only and Android user data persistent.
3. Establish required Binder devices, narrowly scoped GPU access,
   networking/DNS, permissions, cgroups, and private mounts.
4. Boot to a shell and inspect service logs before attempting graphics.
5. Use the selected guest graphics integration for one visible session.
6. Implement graceful stop, failed-start rollback, and restart recovery.

Done: install and launch the initial apps; stop and restart without losing
data or leaving owned processes, mounts, or devices behind. Exercise a
failed boot. Never infer cleanup success from a destructor alone.

### 3. Make apps behave like desktop windows

1. Prove that the guest can expose two independent app tasks/windows.
2. Reuse that guest integration; add a custom bridge only if required.
3. Implement independent surfaces, titles, app IDs, focus, resize, maximize,
   fullscreen, and close behavior. Define close versus Android backgrounding.
4. Handle scaling, rotation, keyboard layout, text input/IME, and correct
   pointer coordinates. Test popups and non-resizable apps explicitly.
5. Generate and remove desktop entries safely as packages change.

Done: two apps launch from the desktop, resize and receive input separately,
and appear correctly in the task switcher. Record unsupported app behaviors.

If custom graphics IPC is necessary, validate peer identity, packet sizes,
descriptor counts, dimensions, planes, offsets, strides, and modifiers.
Handle truncated messages, disconnects, buffer releases, and fences.
Do not copy the original example's unchecked metadata decoding.

### 4. Complete everyday desktop integration

1. Add playback, microphone permission handling, and device changes using
   compatible guest audio components.
2. Add focused text clipboard exchange and notifications.
3. Share only selected folders, with Android storage semantics and correct
   ownership. Check persistence, access revocation, and accidental exposure.
4. Verify usable connectivity through Android's network APIs; add Wi-Fi
   emulation only for an observed supported-app requirement.

Done: the initial apps support their expected everyday workflows, including
text entry, copy/paste, sound, notifications, and selected file access.
This is the first useful desktop release.

### 5. Add ARM compatibility if the target apps need it

1. Identify the required ABIs and a compatible, permitted translator source.
2. Match translator, guest release, native bridge API, and library set.
3. Advertise only ABIs actually supported. Add binfmt rules only when the
   chosen bridge requires them, with explicit ownership and removal.
4. Test one ARM app first; then each target app and its subprocesses.

Done: verified ARM execution and measured additional CPU/RAM costs.
Translator availability or licensing may block this phase.

### 6. Add hardware features individually

Implement camera, gamepad, and Bluetooth only in the order needed by the
target apps. For each, verify the guest HAL, permissions, hotplug, and
ownership model. Do not assume the host and guest can own one Bluetooth
adapter simultaneously. Host gamepad forwarding may avoid guest Bluetooth.

Done: each claimed capability has a real-device check and a clear limitation.

### 7. Add game controls

1. First verify the game's graphics, ABI, and compatibility without mapping.
2. Add focused keyboard/mouse capture and an obvious release action.
3. Implement Type B touch, coordinate transforms, and bounded slot handling.
4. Add WASD, mouse-look, and one small per-game profile.
5. Release all touches on focus loss, disconnect, stop, or cancellation.

Done: simultaneous move/aim/tap works through resize and focus changes;
no stuck touches. Calibrate gestures on the real game. Do not assume a
2 ms reset is invisible or impose constant 250 Hz smoothing without evidence.

### 8. Optimize measured bottlenecks

Measure from phase 0 onward; do focused optimization after behavior works.
Check idle wakeups, unnecessary redraws, queues, CPU pixel copies, GPU
synchronization, and hidden-window activity before adding complexity.
Negotiate explicit sync only where supported. Measure high-refresh modes
against CPU use, frame times, and power; maximum FPS is not minimum CPU.

Done: repeatable improvement against the same workload, with no correctness
regression. State precisely whether savings concern the Rust controller,
Android processes, or the complete system.

### 9. Add macros, editor, and packaging

Build macros and the visual editor only after game controls work. Keep
cancellation and touch cleanup correct. A deadline queue may suffice;
choose an async runtime only when the actual workload benefits.
Prefer an overlay attached to a surface we own over global window tracking.

Then package installation, upgrades, data backup, and uninstall behavior.
Test upgrade failure and data preservation before publishing a release.

## Measurement agreement

Use the same host, image, app versions, resolution, refresh rate, and workload
for comparisons. Repeat runs and record warm-up and measurement duration.

Measure these states: stopped, Android idle, one app active, two apps active,
graphics workload, hidden windows, and repeated start/stop cycles.

- CPU time over a fixed interval, with the percentage convention recorded.
- Host controller RSS/PSS separately from Android; cgroup memory for the
  container. Do not sum RSS and call it unique memory.
- GPU memory and utilization where available, since DMA-BUF memory is not
  fully described by a Rust process's RSS.
- Wakeups, frame-time percentiles, launch time, and persistent resource growth.

Set numeric budgets after the baseline. Target negligible controller work
when idle, bounded resources, and no accumulating leaks. Do not promise an
arbitrary total RAM figure before measuring Android and the target apps.

## How each lesson works

1. Explain one outcome and the Linux/Rust concept needed for it.
2. Name exact files and callers affected; supply short code when requested.
3. You type the change and run the provided focused checks.
4. I review your diff or logs and explain the smallest root-cause fix.
5. You make one Conventional Commit after checks pass.

No automatic source edits or commits during coaching. Build checks become
applicable once Cargo manifests exist. Never count unrun checks as passed.

First lesson: collect the phase 0 host facts and select the initial app set.
No container setup, dependencies, or privileged commands are needed today.

[binder]: https://docs.kernel.org/6.0/admin-guide/binderfs.html
[drop]: https://doc.rust-lang.org/reference/destructors.html
[hwc]: https://source.android.com/docs/core/graphics/implement-hwc
[verified]: https://source.android.com/docs/security/features/verifiedboot
