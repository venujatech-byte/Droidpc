use std::{env, fs, io, process::ExitCode};

fn main() -> io::Result<ExitCode> {
    let mut args = env::args_os().skip(1);

    let is_doctor =
        args.next().as_deref() == Some(std::ffi::OsStr::new("doctor"));

    if !is_doctor || args.next().is_some() {
        eprintln!("Usage: droidpc-cli doctor");
        return Ok(ExitCode::FAILURE);
    }

    let kernel = fs::read_to_string("/proc/sys/kernel/osrelease")?;

    println!("Droidpc");
    println!("Build OS: {}", env::consts::OS);
    println!("Build architecture: {}", env::consts::ARCH);
    println!("Running kernel: {}", kernel.trim());

    Ok(ExitCode::SUCCESS)
}
