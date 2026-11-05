use crate::{ImageSpec, loop_device::LoopDevice};
use std::ffi::CString;
use std::fs;
use std::io;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;

const FLAGS: libc::c_ulong =
    (libc::MS_RDONLY | libc::MS_NOSUID | libc::MS_NODEV) as libc::c_ulong;

/// An owned read-only ext4 image mount. Call from a single-threaded manager;
/// construction changes the calling thread's mount namespace.
pub struct ImageMount {
    path: PathBuf,
    target: CString,
    device: Option<LoopDevice>,
    mounted: bool,
    directory_owned: bool,
    _thread_bound: std::marker::PhantomData<Rc<()>>,
}

impl ImageMount {
    /// `target` must be a new child of a trusted, manager-owned directory.
    pub fn mount(
        images: &Path,
        spec: &ImageSpec,
        target: &Path,
    ) -> io::Result<Self> {
        let target_c = CString::new(target.as_os_str().as_encoded_bytes())
            .map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidInput, "NUL in mount path")
            })?;
        if !target.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "mount path must be absolute",
            ));
        }
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
        fs::DirBuilder::new().mode(0o700).create(target)?;
        let mut owner = Self {
            path: target.to_path_buf(),
            target: target_c,
            device: None,
            mounted: false,
            directory_owned: true,
            _thread_bound: std::marker::PhantomData,
        };
        let setup = (|| {
            owner.device = Some(LoopDevice::attach(images, spec)?);
            let source = CString::new(
                owner
                    .device
                    .as_ref()
                    .unwrap()
                    .path()
                    .as_os_str()
                    .as_encoded_bytes(),
            )
            .expect("generated loop device path");
            if unsafe {
                libc::mount(
                    source.as_ptr(),
                    owner.target.as_ptr(),
                    c"ext4".as_ptr(),
                    FLAGS,
                    c"norecovery".as_ptr().cast(),
                )
            } != 0
            {
                return Err(io::Error::last_os_error());
            }
            owner.mounted = true;
            Ok(())
        })();
        if let Err(error) = setup {
            return match owner.cleanup() {
                Ok(()) => Err(error),
                Err(cleanup) => Err(io::Error::new(
                    cleanup.kind(),
                    format!("{error}; image mount cleanup failed: {cleanup}"),
                )),
            };
        }
        Ok(owner)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn cleanup(mut self) -> io::Result<()> {
        self.release()
    }

    fn release(&mut self) -> io::Result<()> {
        if self.mounted {
            if unsafe { libc::umount2(self.target.as_ptr(), 0) } != 0 {
                return Err(io::Error::last_os_error());
            }
            self.mounted = false;
        }
        if let Some(device) = self.device.take() {
            device.cleanup()?;
        }
        if self.directory_owned {
            fs::remove_dir(&self.path)?;
            self.directory_owned = false;
        }
        Ok(())
    }
}

impl Drop for ImageMount {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_targets_before_privileged_operations() {
        for path in ["relative", "/tmp/nul\0path"] {
            let error = ImageMount::mount(
                Path::new("/unused"),
                &crate::SYSTEM,
                Path::new(path),
            )
            .err()
            .expect("invalid target must fail");
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        }
    }
}
