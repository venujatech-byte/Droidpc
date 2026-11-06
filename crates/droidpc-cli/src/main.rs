mod doctor;
mod launch;

use std::{env, ffi::OsString, path::PathBuf, process::ExitCode};

fn image_directory(
    mut args: impl Iterator<Item = OsString>,
) -> Option<PathBuf> {
    if args.next()?.as_os_str() != "doctor" {
        return None;
    }
    let path = match args.next() {
        None => return Some("/var/lib/droidpc/images".into()),
        Some(flag) if flag == "--images" => PathBuf::from(args.next()?),
        Some(_) => return None,
    };
    if path.as_os_str().is_empty() || args.next().is_some() {
        return None;
    }
    Some(path)
}

fn main() -> ExitCode {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() == 1 && args[0] == "launch" {
        return launch::run();
    }
    match image_directory(env::args_os().skip(1)) {
        Some(images) => doctor::run(&images),
        None => {
            eprintln!("Usage: droidpc-cli doctor [--images DIR] | launch");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_doctor_and_an_optional_image_directory() {
        let parse =
            |args: &[&str]| image_directory(args.iter().map(OsString::from));
        assert_eq!(parse(&["doctor"]), Some("/var/lib/droidpc/images".into()));
        assert_eq!(
            parse(&["doctor", "--images", "/tmp/my images"]),
            Some("/tmp/my images".into())
        );
        for args in [
            vec![],
            vec!["unknown"],
            vec!["doctor", "extra"],
            vec!["doctor", "--images"],
            vec!["doctor", "--images", ""],
            vec!["doctor", "--images", "/tmp", "extra"],
        ] {
            assert_eq!(parse(&args), None);
        }
    }
}
