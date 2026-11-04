use std::env;
use std::fs::{self, File};
use std::io::{self, Read};
use std::os::unix::fs::FileTypeExt;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

#[derive(Default)]
struct Report {
    problems: usize,
    warnings: usize,
}

impl Report {
    fn item(&mut self, status: &str, name: &str, detail: impl AsRef<str>) {
        match status {
            "FAIL" | "UNKNOWN" => self.problems += 1,
            "WARN" => self.warnings += 1,
            _ => {}
        }
        println!("[{status}] {name}: {}", detail.as_ref());
    }

    fn read(&mut self, name: &str, path: &Path) -> Option<String> {
        match fs::read_to_string(path) {
            Ok(value) => Some(value),
            Err(error) => {
                self.item(
                    "UNKNOWN",
                    name,
                    format!("{}: {error}", path.display()),
                );
                None
            }
        }
    }
}

pub fn run(images: &Path) -> ExitCode {
    let mut report = Report::default();
    println!("Droidpc doctor — read-only checks in this process's environment");
    report.item("INFO", "Build OS", env::consts::OS);
    report.item("INFO", "Build architecture", env::consts::ARCH);
    if env::consts::OS != "linux" || env::consts::ARCH != "x86_64" {
        report.item("FAIL", "Platform", "initial target is x86_64 Linux");
    }
    if let Some(kernel) =
        report.read("Kernel", Path::new("/proc/sys/kernel/osrelease"))
    {
        report.item("INFO", "Kernel", kernel.trim());
    }
    if let Some(memory) = report.read("Memory", Path::new("/proc/meminfo")) {
        for line in memory.lines().filter(|line| {
            line.starts_with("MemTotal:") || line.starts_with("MemAvailable:")
        }) {
            report.item("INFO", "Memory", line);
        }
    }
    filesystems(&mut report);
    namespaces(&mut report);
    subordinate_ids(&mut report);
    cgroups(&mut report);
    wayland(&mut report);
    gpu(&mut report);
    security(&mut report);
    for name in ["system.img", "vendor.img"] {
        image(&mut report, &images.join(name));
    }
    report.item(
        "WARN",
        "Image trust",
        "checksums and matched versions unverified",
    );
    report.item(
        "WARN",
        "Runtime",
        "mounts, Binder allocation, namespace creation,",
    );
    println!(
        "       cgroup delegation, guest HALs and Android boot not tested."
    );
    println!("       No modules loaded, mounts created or limits changed.");
    println!(
        "Summary: {} problems, {} warnings",
        report.problems, report.warnings
    );
    if report.problems == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn filesystems(report: &mut Report) {
    if let Some(list) =
        report.read("Filesystems", Path::new("/proc/filesystems"))
    {
        for name in ["binder", "overlay", "ext4"] {
            if has_filesystem(&list, name) {
                report.item("PASS", name, "filesystem registered");
            } else {
                report.item(
                    "WARN",
                    name,
                    "not registered; check kernel/module support",
                );
            }
        }
    }
    match fs::metadata("/dev/loop-control") {
        Ok(meta) if meta.file_type().is_char_device() => {
            report.item(
                "PASS",
                "Loop control",
                "device visible; access untested",
            );
        }
        Ok(_) => report.item("FAIL", "Loop control", "not a character device"),
        Err(error) => report.item(
            "WARN",
            "Loop control",
            format!("{error}; check loop module and device visibility"),
        ),
    }
}

fn namespaces(report: &mut Report) {
    for name in ["mnt", "pid", "uts", "ipc", "net", "user", "cgroup"] {
        let path = format!("/proc/self/ns/{name}");
        match fs::read_link(&path) {
            Ok(target) => report.item(
                "PASS",
                &format!("Namespace {name}"),
                target.to_string_lossy(),
            ),
            Err(error) => report.item(
                "UNKNOWN",
                &format!("Namespace {name}"),
                format!("{error}; namespace creation is not tested"),
            ),
        }
    }
}

fn has_subid_range(contents: &str, user: &str) -> bool {
    contents.lines().any(|line| {
        let mut fields = line.split(':');
        let (Some(name), Some(start), Some(count), None) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            return false;
        };
        let (Ok(start), Ok(count)) =
            (start.parse::<u32>(), count.parse::<u32>())
        else {
            return false;
        };
        name == user
            && count >= 65_535
            && u64::from(start) + u64::from(count) <= u64::from(u32::MAX) + 1
    })
}

fn subordinate_ids(report: &mut Report) {
    let Some(user) = env::var("USER").ok().filter(|name| !name.is_empty())
    else {
        report.item("WARN", "Subordinate IDs", "USER is unset");
        return;
    };
    for (label, path) in [
        ("Subordinate UIDs", "/etc/subuid"),
        ("Subordinate GIDs", "/etc/subgid"),
    ] {
        let Some(contents) = report.read(label, Path::new(path)) else {
            continue;
        };
        let (status, detail) = if has_subid_range(&contents, &user) {
            ("PASS", format!("USER={user} has at least 65535 IDs"))
        } else {
            ("WARN", format!("no 65535-ID range for USER={user}"))
        };
        report.item(status, label, detail);
    }
    report.item(
        "WARN",
        "User namespace mapping",
        "range presence only; setgroups and mapping untested",
    );
}

fn has_filesystem(list: &str, name: &str) -> bool {
    list.lines()
        .any(|line| line.split_whitespace().last() == Some(name))
}

fn has_controller(list: &str, name: &str) -> bool {
    list.split_whitespace().any(|entry| entry == name)
}

fn mount_path(field: &str) -> Option<PathBuf> {
    let mut result = String::new();
    let mut chars = field.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            result.push(ch);
            continue;
        }
        let escape: String = chars.by_ref().take(3).collect();
        result.push(match escape.as_str() {
            "040" => ' ',
            "011" => '\t',
            "012" => '\n',
            "134" => '\\',
            _ => return None,
        });
    }
    let path = PathBuf::from(result);
    safe_absolute(&path).then_some(path)
}

fn safe_absolute(path: &Path) -> bool {
    path.is_absolute()
        && !path.components().any(|part| part == Component::ParentDir)
}

fn cgroup_mounts(mountinfo: &str) -> Vec<(PathBuf, PathBuf)> {
    mountinfo
        .lines()
        .filter_map(|line| {
            let (left, right) = line.split_once(" - ")?;
            if right.split_whitespace().next()? != "cgroup2" {
                return None;
            }
            let mut fields = left.split_whitespace().skip(3);
            Some((mount_path(fields.next()?)?, mount_path(fields.next()?)?))
        })
        .collect()
}

fn current_cgroup(membership: &str) -> Option<&Path> {
    let value = membership
        .lines()
        .find_map(|line| line.strip_prefix("0::"))?;
    let path = Path::new(value);
    safe_absolute(path).then_some(path)
}

fn cgroups(report: &mut Report) {
    let Some(mountinfo) =
        report.read("Cgroup mounts", Path::new("/proc/self/mountinfo"))
    else {
        return;
    };
    let Some(membership) =
        report.read("Cgroup membership", Path::new("/proc/self/cgroup"))
    else {
        return;
    };
    let Some(current) = current_cgroup(&membership) else {
        report.item("UNKNOWN", "Cgroup v2", "no usable v2 membership found");
        return;
    };
    report.item("INFO", "Current cgroup", current.to_string_lossy());
    let mounts = cgroup_mounts(&mountinfo);
    if mounts.len() != 1 {
        report.item(
            "UNKNOWN",
            "Cgroup v2 mount",
            "expected one visible mount; missing or ambiguous layout",
        );
        return;
    }
    let (root, mount) = &mounts[0];
    report.item("INFO", "Cgroup v2 mount", mount.to_string_lossy());
    controllers(report, "Visible root", mount);
    match current.strip_prefix(root) {
        Ok(relative) => {
            let directory = mount.join(relative);
            controllers(report, "Current cgroup", &directory);
            if let Some(enabled) = report.read(
                "Child controllers",
                &directory.join("cgroup.subtree_control"),
            ) {
                let value = enabled.trim();
                report.item(
                    "INFO",
                    "Enabled for children",
                    if value.is_empty() { "none" } else { value },
                );
            }
        }
        Err(_) => report.item(
            "UNKNOWN",
            "Current cgroup directory",
            "membership is outside the visible mount root",
        ),
    }
    report.item(
        "WARN",
        "Cgroup permissions",
        "delegation not tested; controller availability is not write access",
    );
}

fn controllers(report: &mut Report, label: &str, directory: &Path) {
    if let Some(list) =
        report.read(label, &directory.join("cgroup.controllers"))
    {
        for name in ["cpu", "memory", "pids"] {
            let status = if has_controller(&list, name) {
                "PASS"
            } else {
                "WARN"
            };
            let detail = if status == "PASS" {
                "available"
            } else {
                "not listed; check parent delegation or kernel support"
            };
            report.item(status, &format!("{label} {name}"), detail);
        }
    }
}

fn wayland_path(display: &Path, runtime: Option<&Path>) -> Option<PathBuf> {
    if display.as_os_str().is_empty() {
        return None;
    }
    if safe_absolute(display) {
        return Some(display.to_owned());
    }
    let mut parts = display.components();
    if !matches!(parts.next(), Some(Component::Normal(_)))
        || parts.next().is_some()
    {
        return None;
    }
    let runtime = runtime.filter(|path| safe_absolute(path))?;
    Some(runtime.join(display))
}

fn wayland(report: &mut Report) {
    if env::var_os("WAYLAND_SOCKET").is_some() {
        report.item("WARN", "Wayland", "inherited socket FD not inspected");
        return;
    }
    let display = env::var_os("WAYLAND_DISPLAY").unwrap_or("wayland-0".into());
    let runtime = env::var_os("XDG_RUNTIME_DIR");
    let path =
        wayland_path(Path::new(&display), runtime.as_deref().map(Path::new));
    let Some(path) = path else {
        report.item(
            "UNKNOWN",
            "Wayland",
            "check WAYLAND_DISPLAY/XDG_RUNTIME_DIR",
        );
        return;
    };
    match fs::metadata(&path) {
        Ok(meta) if meta.file_type().is_socket() => {
            report.item("PASS", "Wayland socket", path.to_string_lossy());
            report.item(
                "WARN",
                "Wayland protocols",
                "connection, DMA-BUF and synchronization support not tested",
            );
        }
        Ok(_) => report.item("FAIL", "Wayland", "display path is not a socket"),
        Err(error) => report.item(
            "UNKNOWN",
            "Wayland",
            format!("{}: {error}; run in your Wayland session", path.display()),
        ),
    }
}

fn gpu(report: &mut Report) {
    let entries = match fs::read_dir("/dev/dri") {
        Ok(entries) => entries,
        Err(error) => {
            report.item("UNKNOWN", "GPU", format!("/dev/dri: {error}"));
            return;
        }
    };
    let mut found = false;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                report.item("UNKNOWN", "GPU device", error.to_string());
                continue;
            }
        };
        if !entry.file_name().to_string_lossy().starts_with("renderD") {
            continue;
        }
        found = true;
        let path = entry.path();
        let result = fs::metadata(&path).and_then(|meta| {
            if !meta.file_type().is_char_device() {
                return Err(io::Error::other("not a character device"));
            }
            File::options().read(true).write(true).open(&path)
        });
        match result {
            Ok(_) => report.item(
                "PASS",
                "GPU read/write open",
                path.to_string_lossy(),
            ),
            Err(error) => report.item(
                "UNKNOWN",
                "GPU access",
                format!("{}: {error}", path.display()),
            ),
        }
    }
    if !found {
        report.item("UNKNOWN", "GPU", "no render nodes visible");
    }
    report.item(
        "WARN",
        "GPU compatibility",
        "guest driver and rendering untested",
    );
}

fn security(report: &mut Report) {
    for (name, path) in [
        ("SELinux enforcing", "/sys/fs/selinux/enforce"),
        (
            "AppArmor enabled",
            "/sys/module/apparmor/parameters/enabled",
        ),
    ] {
        match fs::read_to_string(path) {
            Ok(value) => report.item("INFO", name, value.trim()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                report.item("INFO", name, "interface not visible");
            }
            Err(error) => report.item("UNKNOWN", name, error.to_string()),
        }
    }
    report.item(
        "WARN",
        "Security policy",
        "container permissions untested; keep host security enabled",
    );
}

fn image_format(header: &[u8]) -> Option<&'static str> {
    if header.starts_with(&[0x3a, 0xff, 0x26, 0xed]) {
        Some("Android sparse")
    } else if header.get(1080..1082) == Some(&[0x53, 0xef]) {
        Some("ext filesystem")
    } else {
        None
    }
}

fn image(report: &mut Report, path: &Path) {
    let name = path.display().to_string();
    let result = (|| -> io::Result<(u64, Vec<u8>)> {
        if !fs::metadata(path)?.is_file() {
            return Err(io::Error::other("expected a regular image file"));
        }
        let file = File::open(path)?;
        let size = file.metadata()?.len();
        let mut header = Vec::with_capacity(1082);
        file.take(1082).read_to_end(&mut header)?;
        Ok((size, header))
    })();
    match result {
        Ok((size, header)) => match image_format(&header) {
            Some("Android sparse") => report.item(
                "WARN",
                &name,
                "sparse signature; conversion needed before direct mounting",
            ),
            Some(format) => report.item(
                "PASS",
                &name,
                format!("{size} bytes; {format} signature, not validation"),
            ),
            None => report.item(
                "UNKNOWN",
                &name,
                "unrecognized/truncated image; check format and download",
            ),
        },
        Err(error) => report.item(
            "UNKNOWN",
            &name,
            format!("{error}; select images with --images DIR"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_exactly() {
        assert!(has_filesystem("nodev\tbinder\n\text4\n", "binder"));
        assert!(!has_filesystem("nodev binder_extra", "binder"));
        assert!(!has_filesystem("", "binder"));
        for name in ["cpu", "memory", "pids"] {
            assert!(has_controller("cpu\tmemory  pids\n", name));
        }
        assert!(!has_controller("cpuset memory_extra", "cpu"));
        assert!(!has_controller("", "memory"));
    }

    #[test]
    fn subordinate_ranges_require_an_exact_user_and_sufficient_count() {
        let entries = "ann:524288:65536\nanna:100000:65535\n";
        assert!(has_subid_range(entries, "ann"));
        assert!(has_subid_range(entries, "anna"));
        assert!(!has_subid_range(entries, "an"));
        assert!(!has_subid_range("ann:1:65534\n", "ann"));
        assert!(!has_subid_range("ann:4294967295:65535\n", "ann"));
        assert!(!has_subid_range("ann:1:65535:extra\n", "ann"));
    }

    #[test]
    fn cgroup_paths_handle_mount_roots_and_escapes() {
        let mounts = cgroup_mounts(
            "1 0 0:1 /user.slice /custom\\040group rw - cgroup2 none rw\n",
        );
        assert_eq!(
            mounts,
            vec![("/user.slice".into(), "/custom group".into())]
        );
        let group = current_cgroup("1:cpu:/old\n0::/user.slice/app\n").unwrap();
        assert_eq!(
            mounts[0].1.join(group.strip_prefix(&mounts[0].0).unwrap()),
            Path::new("/custom group/app")
        );
        assert!(current_cgroup("0::/../escape").is_none());
        assert!(current_cgroup("1:cpu:/old").is_none());
        assert!(mount_path("/bad\\099").is_none());
        assert!(cgroup_mounts("bad input").is_empty());
    }

    #[test]
    fn wayland_paths_require_a_valid_runtime_for_relative_names() {
        let runtime = Some(Path::new("/run/user/1000"));
        assert_eq!(
            wayland_path(Path::new("wayland-1"), runtime),
            Some("/run/user/1000/wayland-1".into())
        );
        assert_eq!(
            wayland_path(Path::new("/tmp/socket"), None),
            Some("/tmp/socket".into())
        );
        assert!(wayland_path(Path::new("wayland-0"), None).is_none());
        assert!(wayland_path(Path::new("../socket"), runtime).is_none());
        assert!(wayland_path(Path::new(".."), runtime).is_none());
        assert!(wayland_path(Path::new("."), runtime).is_none());
        assert!(wayland_path(Path::new(""), runtime).is_none());
    }

    #[test]
    fn image_signatures_do_not_panic_on_short_files() {
        assert_eq!(image_format(&[]), None);
        assert_eq!(image_format(&[0x3a, 0xff, 0x26]), None);
        assert_eq!(
            image_format(&[0x3a, 0xff, 0x26, 0xed]),
            Some("Android sparse")
        );
        let mut header = vec![0; 1082];
        assert_eq!(image_format(&header), None);
        header[1080..].copy_from_slice(&[0x53, 0xef]);
        assert_eq!(image_format(&header), Some("ext filesystem"));
    }
}
