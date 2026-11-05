use std::ffi::{CString, c_char, c_ulong};
use std::fs;
use std::io;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::rc::Rc;

const BINDERFS_MAX_NAME: usize = 255;
const MOUNT_FLAGS: libc::c_ulong = libc::MS_NOSUID as libc::c_ulong;
const IOC_READ_WRITE: c_ulong = 3;

#[repr(C)]
struct BinderFsDevice {
    name: [c_char; BINDERFS_MAX_NAME + 1],
    major: u32,
    minor: u32,
}

const fn binder_ctl_add() -> c_ulong {
    (IOC_READ_WRITE << 30)
        | ((std::mem::size_of::<BinderFsDevice>() as c_ulong) << 16)
        | ((b'b' as c_ulong) << 8)
        | 1
}

fn combine_cleanup_error(error: io::Error, cleanup: io::Error) -> io::Error {
    io::Error::new(
        cleanup.kind(),
        format!("{error}; BinderFS cleanup failed: {cleanup}"),
    )
}

fn unmount(mountpoint: &Path, target: &CString) -> io::Result<()> {
    if unsafe { libc::umount2(target.as_ptr(), 0) } != 0 {
        return Err(io::Error::last_os_error());
    }
    fs::remove_dir(mountpoint)
}

/// An owned BinderFS mount in a private mount namespace.
///
/// The mount namespace is made private on the calling thread. Use this from
/// Droidpc's single-threaded foreground manager before it starts the guest.
pub struct BinderFs {
    mountpoint: Option<PathBuf>,
    target: CString,
    control: Option<OwnedFd>,
    _thread_bound: std::marker::PhantomData<Rc<()>>,
}

impl BinderFs {
    pub fn mount(mountpoint: &Path) -> io::Result<Self> {
        let mountpoint = mountpoint.to_path_buf();
        let target = CString::new(mountpoint.as_os_str().as_encoded_bytes())
            .map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidInput, "NUL in mount path")
            })?;

        if unsafe { libc::unshare(libc::CLONE_NEWNS) } != 0 {
            return Err(io::Error::last_os_error());
        }
        if unsafe {
            libc::mount(
                std::ptr::null(),
                c"/".as_ptr(),
                std::ptr::null(),
                (libc::MS_REC | libc::MS_PRIVATE) as libc::c_ulong,
                std::ptr::null(),
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }

        fs::create_dir(&mountpoint)?;
        if let Err(error) =
            fs::set_permissions(&mountpoint, fs::Permissions::from_mode(0o700))
        {
            return match fs::remove_dir(&mountpoint) {
                Ok(()) => Err(error),
                Err(cleanup) => Err(combine_cleanup_error(error, cleanup)),
            };
        }
        let source = c"binder";
        let filesystem = c"binder";
        if unsafe {
            libc::mount(
                source.as_ptr(),
                target.as_ptr(),
                filesystem.as_ptr(),
                MOUNT_FLAGS,
                std::ptr::null(),
            )
        } != 0
        {
            let error = io::Error::last_os_error();
            return match fs::remove_dir(&mountpoint) {
                Ok(()) => Err(error),
                Err(cleanup) => Err(combine_cleanup_error(error, cleanup)),
            };
        }

        let control_path = mountpoint.join("binder-control");
        let control = match fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(control_path)
        {
            Ok(file) => OwnedFd::from(file),
            Err(error) => {
                return match unmount(&mountpoint, &target) {
                    Ok(()) => Err(error),
                    Err(cleanup) => Err(combine_cleanup_error(error, cleanup)),
                };
            }
        };

        let binder_fs = Self {
            mountpoint: Some(mountpoint),
            target,
            control: Some(control),
            _thread_bound: std::marker::PhantomData,
        };
        for name in ["binder", "hwbinder", "vndbinder"] {
            if let Err(error) = binder_fs.add_device(name) {
                let error = match binder_fs.cleanup() {
                    Ok(()) => error,
                    Err(cleanup) => combine_cleanup_error(error, cleanup),
                };
                return Err(error);
            }
        }
        Ok(binder_fs)
    }

    pub fn device_path(&self, name: &str) -> io::Result<PathBuf> {
        if !["binder", "hwbinder", "vndbinder"].contains(&name) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unknown Binder device",
            ));
        }
        Ok(self
            .mountpoint
            .as_ref()
            .expect("mounted BinderFS")
            .join(name))
    }

    pub fn cleanup(mut self) -> io::Result<()> {
        if self.mountpoint.is_none() {
            return Ok(());
        }
        drop(self.control.take());
        if unsafe { libc::umount2(self.target.as_ptr(), 0) } != 0 {
            return Err(io::Error::last_os_error());
        }
        fs::remove_dir(self.mountpoint.take().expect("mounted BinderFS"))
    }

    fn add_device(&self, name: &str) -> io::Result<()> {
        let path = self
            .mountpoint
            .as_ref()
            .expect("mounted BinderFS")
            .join(name);
        let mut device = BinderFsDevice {
            name: [0; BINDERFS_MAX_NAME + 1],
            major: 0,
            minor: 0,
        };
        for (dst, src) in device.name.iter_mut().zip(name.bytes()) {
            *dst = src as c_char;
        }
        if unsafe {
            libc::ioctl(
                self.control
                    .as_ref()
                    .expect("open binder-control")
                    .as_raw_fd(),
                binder_ctl_add(),
                &mut device,
            )
        } < 0
        {
            return Err(io::Error::last_os_error());
        }
        if !fs::symlink_metadata(&path)?.file_type().is_char_device() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "BinderFS did not create a character device",
            ));
        }
        fs::set_permissions(&path, fs::Permissions::from_mode(0o666))
    }
}

impl Drop for BinderFs {
    fn drop(&mut self) {
        drop(self.control.take());
        if let Some(mountpoint) = self.mountpoint.take()
            && unsafe { libc::umount2(self.target.as_ptr(), 0) } == 0
        {
            let _ = fs::remove_dir(mountpoint);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binder_ioctl_matches_linux_uapi_layout() {
        assert_eq!(std::mem::size_of::<BinderFsDevice>(), 264);
        assert_eq!(binder_ctl_add(), 0xc108_6201);
    }
}
