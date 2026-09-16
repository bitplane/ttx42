use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ttx42"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn legacy_tti_controls_render_like_escaped_controls() {
    let legacy = run(
        &["--format", "tti"],
        b"PN,10001\r\nOL,1,\x81RED,\x8dTALL\r\n",
    );
    let escaped = run(
        &["--format", "tti"],
        b"PN,10001\r\nOL,1,\x1bARED,\x1bMTALL\r\n",
    );
    assert!(legacy.status.success());
    assert!(escaped.status.success());
    assert_eq!(legacy.stdout, escaped.stdout);
}
