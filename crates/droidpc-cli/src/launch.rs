use std::{env, path::Path, process::ExitCode};

pub fn run() -> ExitCode {
    let root = match env::current_dir() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("Could not find repository directory: {error}");
            return ExitCode::FAILURE;
        }
    };
    let script = root.join("scripts/probe-android-init-selinux.sh");
    if !script.is_file()
        || !Path::new("target/release/droidpc-window").is_file()
    {
        eprintln!(
            "Run from the repository root after: cargo build --release -p droidpc-cli -p droidpc-window"
        );
        return ExitCode::FAILURE;
    }
    #[cfg(unix)]
    {
        use std::{os::unix::process::CommandExt, process::Command};
        let error = Command::new("sudo")
            .arg("env")
            .args(["DROIDPC_PROBE_HOST_CAPS=1", "DROIDPC_PROBE_WINDOW=1"])
            .arg(format!(
                "DROIDPC_WINDOW_FPS={}",
                env::var("DROIDPC_WINDOW_FPS").unwrap_or_else(|_| "60".into())
            ))
            .arg(format!(
                "DROIDPC_CAPTURE_SOURCE={}",
                env::var("DROIDPC_CAPTURE_SOURCE")
                    .unwrap_or_else(|_| "snapshot".into())
            ))
            .arg(script)
            .exec();
        eprintln!("Could not launch Android: {error}");
    }
    #[cfg(not(unix))]
    eprintln!("Android container launch requires Linux");
    ExitCode::FAILURE
}
