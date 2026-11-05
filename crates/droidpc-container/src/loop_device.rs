use crate::{ImageSpec, open_verified};
use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

// Linux UAPI: linux/loop.h. LOOP_CONFIGURE sets flags atomically.
const LOOP_CONFIGURE: libc::c_ulong = 0x4c0a;
const LOOP_CLR_FD: libc::c_ulong = 0x4c01;
const LOOP_CTL_GET_FREE: libc::c_ulong = 0x4c82;
const READ_ONLY_AUTOCLEAR: u32 = 1 | 4;

#[repr(C)]
struct LoopInfo {
    device: u64,
    inode: u64,
    rdevice: u64,
    offset: u64,
    size_limit: u64,
    number: u32,
    encrypt_type: u32,
    encrypt_key_size: u32,
    flags: u32,
    file_name: [u8; 64],
    crypt_name: [u8; 64],
    encrypt_key: [u8; 32],
    init: [u64; 2],
}

#[repr(C)]
struct LoopConfig {
    fd: u32,
    block_size: u32,
    info: LoopInfo,
    reserved: [u64; 8],
}

/// A read-only, autoclearing loop device backed by a pinned verified image.
/// Unmount users of this device before calling cleanup.
pub struct LoopDevice {
    device: Option<File>,
    path: PathBuf,
    _image: File,
}

impl LoopDevice {
    pub fn attach(directory: &Path, spec: &ImageSpec) -> io::Result<Self> {
        let image = open_verified(directory, spec)?;
        let control = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/loop-control")?;
        let config = LoopConfig {
            fd: image.as_raw_fd() as u32,
            block_size: 0,
            info: LoopInfo {
                flags: READ_ONLY_AUTOCLEAR,
                // All fields are integer values or byte arrays.
                ..unsafe { std::mem::zeroed() }
            },
            reserved: [0; 8],
        };
        // GET_FREE does not reserve the device; another process may win.
        for _ in 0..16 {
            let number =
                unsafe { libc::ioctl(control.as_raw_fd(), LOOP_CTL_GET_FREE) };
            if number < 0 {
                return Err(io::Error::last_os_error());
            }
            let path = PathBuf::from(format!("/dev/loop{number}"));
            let device =
                OpenOptions::new().read(true).write(true).open(&path)?;
            if unsafe {
                libc::ioctl(device.as_raw_fd(), LOOP_CONFIGURE, &config)
            } == 0
            {
                return Ok(Self {
                    device: Some(device),
                    path,
                    _image: image,
                });
            }
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::EBUSY) {
                return Err(error);
            }
        }
        Err(io::Error::from_raw_os_error(libc::EBUSY))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn cleanup(mut self) -> io::Result<()> {
        self.detach()
    }

    fn detach(&mut self) -> io::Result<()> {
        if let Some(device) = &self.device {
            if unsafe { libc::ioctl(device.as_raw_fd(), LOOP_CLR_FD) } != 0 {
                return Err(io::Error::last_os_error());
            }
            self.device.take();
        }
        Ok(())
    }
}

impl Drop for LoopDevice {
    fn drop(&mut self) {
        // Closing the last fd also invokes the kernel's autoclear fallback.
        let _ = self.detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_matches_linux_uapi() {
        assert_eq!(std::mem::size_of::<LoopInfo>(), 232);
        assert_eq!(std::mem::size_of::<LoopConfig>(), 304);
        assert_eq!(std::mem::offset_of!(LoopInfo, flags), 52);
        assert_eq!(std::mem::offset_of!(LoopConfig, info), 8);
    }
}
