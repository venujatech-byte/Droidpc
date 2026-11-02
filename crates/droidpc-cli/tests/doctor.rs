use std::process::Command;

#[test]
fn missing_images_do_not_stop_the_report() {
    let output = Command::new(env!("CARGO_BIN_EXE_droidpc-cli"))
        .args(["doctor", "--images", "/dev/null/not-a-directory"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let report = String::from_utf8(output.stdout).unwrap();
    assert!(report.contains("system.img"));
    assert!(report.contains("vendor.img"));
    assert!(report.contains("[WARN] Runtime:"));
    assert!(report.contains("Summary:"));
    assert!(output.stderr.is_empty());
}

#[test]
fn invalid_arguments_do_not_run_diagnostics() {
    for args in [vec![], vec!["doctor", "extra"], vec!["doctor", "--images"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_droidpc-cli"))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8(output.stderr).unwrap().contains("Usage:"));
    }
}

#[test]
fn non_utf8_arguments_do_not_panic() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let output = Command::new(env!("CARGO_BIN_EXE_droidpc-cli"))
        .arg(OsString::from_vec(vec![0xff]))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}
