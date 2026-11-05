use droidpc_container::binder::BinderFs;
use std::fs::{self, OpenOptions};
use std::io;
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, PermissionsExt};
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

fn check(binder: &BinderFs) -> io::Result<()> {
    for name in ["binder", "hwbinder", "vndbinder"] {
        let path = binder.device_path(name)?;
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.file_type().is_char_device() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{name} is not a character device"),
            ));
        }
        if metadata.permissions().mode() & 0o777 != 0o666 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{name} has unexpected permissions"),
            ));
        }
        OpenOptions::new().read(true).write(true).open(&path)?;
        println!("PASS {name}: private character device opens");
    }
    Ok(())
}

fn run(mountpoint: &Path) -> io::Result<()> {
    let binder = BinderFs::mount(mountpoint)?;
    let mut result = check(&binder);
    record(&mut result, binder.cleanup());
    result
}

fn main() -> io::Result<()> {
    if unsafe { libc::geteuid() } != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "BinderFS staging requires root",
        ));
    }
    let mut args = env::args_os().skip(1);
    if args.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: binder (requires root)",
        ));
    }
    let root = (0..1000)
        .find_map(|attempt| {
            let path = PathBuf::from(format!(
                "/tmp/droidpc-binder-{}-{attempt}",
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
                "no unused BinderFS temp directory",
            ))
        })?;
    let mountpoint = root.join("binderfs");
    let mut result = run(&mountpoint);
    record(&mut result, fs::remove_dir(&root));
    result?;
    println!("Private BinderFS devices cleaned up");
    Ok(())
}
