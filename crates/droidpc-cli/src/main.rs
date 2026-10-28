use std::{env, fs, io};

fn main() -> io::Result<()> {
    let kernel = fs::read_to_string("/proc/sys/kernel/osrelease")?;

    println!("Droidpc");
    println!("Build OS: {}", env::consts::OS);
    println!("Build architecture: {}", env::consts::ARCH);
    println!("Running kernel: {}", kernel.trim());

    Ok(())
}
