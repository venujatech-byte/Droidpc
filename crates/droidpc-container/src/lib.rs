pub mod binder;

use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

pub struct ImageSpec {
    pub name: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

pub const SYSTEM: ImageSpec = ImageSpec {
    name: "system.img",
    size: 1_827_717_120,
    sha256: concat!(
        "30c87ee44412fee4f50173716a6bb7c6941d",
        "3021853f0adab4a249da8a9455ab",
    ),
};

pub const VENDOR: ImageSpec = ImageSpec {
    name: "vendor.img",
    size: 562_388_992,
    sha256: concat!(
        "5cc4267517ded28f1cf60a5d2b6e8b88b15d",
        "8520de1f55a39de7764e7929a877",
    ),
};

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn verify_bytes(mut input: impl Read, spec: &ImageSpec) -> io::Result<()> {
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut count = 0_u64;

    loop {
        let length = match input.read(&mut buffer) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                continue;
            }
            result => result?,
        };
        if length == 0 {
            break;
        }
        count = count
            .checked_add(length as u64)
            .ok_or_else(|| invalid("image size overflow"))?;
        if count > spec.size {
            return Err(invalid("image is larger than expected"));
        }
        hash.update(&buffer[..length]);
    }

    if count != spec.size {
        return Err(invalid("image size mismatch"));
    }
    if format!("{:x}", hash.finalize()) != spec.sha256 {
        return Err(invalid("image SHA-256 mismatch"));
    }
    Ok(())
}

pub fn open_verified(directory: &Path, spec: &ImageSpec) -> io::Result<File> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(directory.join(spec.name))?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() != spec.size {
        return Err(invalid("expected a regular image of the pinned size"));
    }
    verify_bytes(&mut file, spec)?;
    file.rewind()?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ABC: ImageSpec = ImageSpec {
        name: "fixture",
        size: 3,
        sha256: concat!(
            "ba7816bf8f01cfea414140de5dae2223",
            "b00361a396177a9cb410ff61f20015ad",
        ),
    };

    #[test]
    fn accepts_known_bytes_and_rejects_corruption_and_wrong_sizes() {
        assert!(verify_bytes(&b"abc"[..], &ABC).is_ok());
        for bytes in [&b"abd"[..], &b"ab"[..], &b"abcd"[..]] {
            let error = verify_bytes(bytes, &ABC).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        }
    }

    #[test]
    fn preserves_read_errors() {
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::from(io::ErrorKind::PermissionDenied))
            }
        }
        assert_eq!(
            verify_bytes(Broken, &ABC).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
    }
}
