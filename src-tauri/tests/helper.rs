#[test]
fn internal_helper_rejects_an_unconfigured_launch() {
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_terminal-pty-helper"))
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(2));
}
