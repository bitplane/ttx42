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

#[test]
fn t42_subpage_selection_without_page_number_is_respected() {
    // Two headers for page 100, subpages 1 and 2, encoded with Hamming 8/4.
    let mut input = Vec::new();
    for subpage in [0x02, 0x49] {
        input.extend([
            0x02, 0x15, 0x15, 0x15, subpage, 0x15, 0x15, 0x15, 0x15, 0x15,
        ]);
        input.extend([b' '; 32]);
    }
    let listing = run(&["--format", "t42"], &input);
    assert!(listing.status.success());
    assert_eq!(listing.stdout, b"100 0001\n100 0002\n");

    let selected = run(&["--format", "t42", "--subpage", "2"], &input);
    let explicit = run(
        &["--format", "t42", "--page", "100", "--subpage", "2"],
        &input,
    );
    assert!(selected.status.success());
    assert!(explicit.status.success());
    assert_eq!(selected.stdout, explicit.stdout);
    assert!(selected.stdout.starts_with(b"\x1b["));

    let missing = run(&["--format", "t42", "--subpage", "F"], &input);
    assert!(!missing.status.success());
    assert!(missing.stdout.is_empty());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("requested page not found"));
}

#[test]
fn tti_detection_accepts_leading_metadata_and_row_only_pages() {
    for input in [
        &b"DE,Blank page\r\nPN,10001\r\nSC,0001\r\n"[..],
        &b"DE,Blank page\nPN,10001\nSC,0001\n"[..],
        &b"OL,1,HELLO\r\n"[..],
    ] {
        let detected = run(&[], input);
        let explicit = run(&["--format", "tti"], input);
        assert!(detected.status.success(), "{:?}", detected.stderr);
        assert!(explicit.status.success());
        assert_eq!(detected.stdout, explicit.stdout);
    }
    // A record marker inside a raw row is ordinary text, not a TTI record.
    let mut raw = [b' '; 1000];
    raw[10..17].copy_from_slice(b"PN,1001");
    let detected = run(&[], &raw);
    let explicit = run(&["--format", "raw"], &raw);
    assert!(detected.status.success());
    assert_eq!(detected.stdout, explicit.stdout);
}
