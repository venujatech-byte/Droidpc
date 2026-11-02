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

    match fs::read_to_string("/proc/filesystems") {
        Ok(filesystems) if has_binderfs(&filesystems) => {
            println!("BinderFS: available");
        }
        Ok(_) => {
            println!("BinderFS: not currently registered");
            println!("Check kernel support or module availability.");
        }
        Err(error) => {
            eprintln!("BinderFS: unknown ({error})");
            return Ok(ExitCode::FAILURE);
        }
    }

    Ok(ExitCode::SUCCESS)
}

fn has_binderfs(filesystems: &str) -> bool {
    filesystems
        .lines()
        .any(|line| line.split_whitespace().last() == Some("binder"))
}

#[cfg(test)]
mod tests {
    use super::has_binderfs;

    #[test]
    fn detects_exact_binder_filesystem_name() {
        assert!(has_binderfs("nodev\tbinder\n"));
        assert!(has_binderfs("nodev   binder  \n\text4\n"));
        assert!(!has_binderfs("nodev\tbinderfs\n"));
        assert!(!has_binderfs("nodev\tbinder_extra\n"));
        assert!(!has_binderfs("\text4\n"));
        assert!(!has_binderfs(""));
    }
}
