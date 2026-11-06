# Guest device inventory

This is a read-only inventory of device paths named by the selected Android
images. It is a starting allowlist for Droidpc design, not evidence that a
device works or that Android has booted. The image pair stays unmodified.

## What the image declares

`/system/etc/ueventd.rc` contains rules for these paths:

| Device path | Why Android may need it | First milestone |
| --- | --- | --- |
| `/dev/null`, `/dev/zero`, `/dev/full`, `/dev/ptmx`, `/dev/tty` | Basic Android processes and terminals. | Provide private standard devices. |
| `/dev/random`, `/dev/urandom` | Random numbers used by system libraries and security code. | Provide private kernel-backed devices. |
| `/dev/binder`, `/dev/hwbinder`, `/dev/vndbinder` | Android framework and HAL IPC. | Allocate private BinderFS devices. |
| `/dev/dri/renderD*` | Hardware rendering through a DRM render node. | Evaluate for the selected GPU. |
| `/dev/dri/card*` | DRM display control, when the guest graphics stack requires it. | Do not pass until the composer needs it. |
| `/dev/dma_heap/*` | Shared graphics buffers on supported kernels. | Determine allocator requirements. |
| `/dev/input/event*` | Keyboard, pointer and touchscreen events. | Add only selected input devices. |
| `/dev/snd/*` | Sound playback and capture. | Add after core Android boot. |
| `/dev/uhid`, `/dev/uinput` | Virtual HID and input injection. | Optional input feature; not needed for first boot. |
| `/dev/video*` | External camera capture. | Optional camera feature. |
| `/dev/tun` | Tunnel interface used by VPN services. | Optional; networking design decides. |
| `/dev/kvm`, `/dev/vhost-vsock` | Virtualization interfaces. | Not part of the initial container goal. |

The stock rules also modify sysfs permissions under CPU frequency and input
paths. Do not expose a writable host `/sys` to let Android apply those
rules. Those paths can affect the host kernel and require a deliberately
scoped guest view or a replacement behavior.

The vendor image's VINTF manifest declares HIDL services for graphics
composer and allocator, audio, external camera, sensors, power, health,
lights, keymaster, gatekeeper, media OMX, and vendor-specific clipboard,
display, and task services. A manifest declaration means Android
expects a service interface. It does not prove that the service starts,
that its device nodes are present, or that a corresponding host bridge
exists.

## What to provide first

1. **Private standard devices.** Bubblewrap already creates a minimal `/dev`
   for probes. A production runtime must confirm the needed nodes and
   permissions inside its own device filesystem.
2. **BinderFS devices.** Mount a private BinderFS and allocate the three
   named devices. Do not borrow device nodes from another Android runtime.
3. **A basic Android shell.** Start with no GPU, sound, camera or host input
   passthrough. Check that init, properties and framework services can run.
4. **Graphics.** Inspect the selected Intel host's DRM render node and the
   guest minigbm/composer configuration. Start with only the render node if
   that stack supports it; grant a card node only if measured requirements
   establish the need.
5. **Input and sound.** Forward individual devices only when the guest
   feature is implemented and permission handling is understood.

The first init probe could not start ueventd: its 16 MiB receive-buffer
request required `SO_RCVBUFFORCE`, which failed with `EPERM`. A temporary
config override requesting 256 KiB let ueventd start and complete coldboot.
The probe's `/sys` is empty and private, so the result does not test physical
device discovery. Do not solve device access by sharing the host network
namespace or forwarding every host uevent. Android ueventd normally listens
to kernel uevent sockets, scans sysfs, creates device nodes, and applies
image-specific permissions.

The socket, all-groups bind, credential option and smaller receive buffer
were checked in a disposable namespace. The kernel reported an effective
512 KiB buffer for the 256 KiB request. The override is only a probe
configuration; production still needs measured event requirements.

After ueventd coldboot, the guest `/dev` listing contained standard device
nodes and Android-created cgroup directories. It contained no Binder, DRM,
input or sound nodes. The probe's `/sys` is empty, so this is expected and
does not describe which devices exist on the host. The next blocker is the
Android cgroup v1/v2 mismatch; resolve it before starting the stock service
sequence. Then add private BinderFS and only the hardware nodes needed by
the first app.

## Current boundary and next investigation

Image source inspected with read-only `debugfs`:

- `/system/etc/ueventd.rc`
- `/vendor/etc/vintf/manifest.xml`
- `/vendor/bin/hw/` service names

The image also has vendor-specific composer, display and task services. Keep
their attribution. Droidpc's host program and any new integration must
be original work; this inventory does not prescribe copying their runtime.

Before writing a uevent forwarder, answer these questions with a focused
test:

1. Can the guest boot its core framework with private standard devices and
   BinderFS while ueventd is absent?
2. Which guest services actually fail because of missing device nodes?
3. Can we create only those nodes from an explicit allowlist, with correct
   major/minor numbers, ownership, permissions and SELinux labels?
4. Which sysfs paths does a required HAL read or write, and can access be
   limited to a private, read-only view?
5. Does the selected graphics stack work with a DRM render node alone?

Until these are answered, Droidpc has no verified device-event bridge and
no Android desktop session. See [the boot guide](START-HERE.md),
[image inspection](IMAGE-BOOT.md), and
[the cgroup/init probe record](CGROUP-PROBE.md).
