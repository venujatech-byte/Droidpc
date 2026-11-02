use droidpc_container::{SYSTEM, VENDOR, open_verified};
use std::{env, io, path::PathBuf};

fn main() -> io::Result<()> {
    let mut args = env::args_os().skip(1);
    let directory = args.next().map(PathBuf::from);
    let Some(directory) = directory.filter(|_| args.next().is_none()) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: verify IMAGE_DIRECTORY",
        ));
    };
    let _system = open_verified(&directory, &SYSTEM)?;
    let _vendor = open_verified(&directory, &VENDOR)?;
    println!("Verified system and vendor; no mounts created");
    Ok(())
}
