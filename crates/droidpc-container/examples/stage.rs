use droidpc_container::{SYSTEM, VENDOR, mounts::ImageMount, open_verified};
use std::fs;
use std::io;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::{env, process};

fn record(result: &mut io::Result<()>, next: io::Result<()>) {
    if let Err(error) = next {
        *result = Err(match result {
            Ok(()) => error,
            Err(previous) => {
                io::Error::new(error.kind(), format!("{previous}; {error}"))
            }
        });
    }
}

fn check(mount: &ImageMount, expected: &str) -> io::Result<()> {
    let path = mount.path();
    if !path.join(expected).is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("missing staged file: {expected}"),
        ));
    }
    let test_path = path.join(".droidpc-readonly-check");
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&test_path)
    {
        Err(error) if error.raw_os_error() == Some(libc::EROFS) => Ok(()),
        Err(error) => Err(error),
        Ok(file) => {
            drop(file);
            fs::remove_file(test_path)?;
            Err(io::Error::other("image mount unexpectedly permits writes"))
        }
    }
}

fn stage(images: &Path, root: &Path) -> io::Result<()> {
    let system = ImageMount::mount(images, &SYSTEM, &root.join("system"))?;
    let vendor = match ImageMount::mount(images, &VENDOR, &root.join("vendor"))
    {
        Ok(vendor) => vendor,
        Err(error) => {
            let mut result = Err(error);
            record(&mut result, system.cleanup());
            return result;
        }
    };
    let mut result = check(&system, "system/build.prop");
    record(&mut result, check(&vendor, "build.prop"));
    record(&mut result, vendor.cleanup());
    record(&mut result, system.cleanup());
    result
}

fn main() -> io::Result<()> {
    let mut args = env::args_os().skip(1);
    let images = args.next().map(PathBuf::from);
    let Some(images) = images.filter(|_| args.next().is_none()) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: stage IMAGE_DIRECTORY (requires root)",
        ));
    };
    if unsafe { libc::geteuid() } != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "image staging requires root",
        ));
    }
    let root = (0..1000)
        .find_map(|attempt| {
            let path = PathBuf::from(format!(
                "/tmp/droidpc-stage-{}-{attempt}",
                process::id()
            ));
            match fs::DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => Some(Ok(path)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    None
                }
                Err(error) => Some(Err(error)),
            }
        })
        .unwrap_or_else(|| {
            Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "no unused staging temp directory",
            ))
        })?;
    let mut result = stage(&images, &root);
    record(&mut result, fs::remove_dir(&root));
    result?;
    // Recheck original bytes after kernel mount and cleanup.
    open_verified(&images, &SYSTEM)?;
    open_verified(&images, &VENDOR)?;
    println!("System/vendor staged read-only, cleaned up, hashes unchanged");
    Ok(())
}
