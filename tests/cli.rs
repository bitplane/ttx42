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
fn file_read_errors_include_the_path_and_os_error() {
    let path = format!(
        "{}/target/ttx42-missing-input-{}/page.tti",
        env!("CARGO_MANIFEST_DIR"),
        std::process::id()
    );
    let expected = std::fs::read(&path).unwrap_err().to_string();
    let result = run(&[&path], b"");
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(error.contains(&format!("{path:?}")), "{error}");
    assert!(error.contains(&expected), "{error}");
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
fn undecodable_t42_input_reports_no_pages() {
    for input in [&[][..], &[0xff; 42][..]] {
        let result = run(&["--format", "t42"], input);
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("input contains no teletext pages")
        );
    }
}

#[test]
fn absent_tti_subpage_matches_zero() {
    let input = b"PN,100\nOL,1,HELLO\n";
    let selected = run(&["--page", "100", "--subpage", "0"], input);
    let unfiltered = run(&["--page", "100"], input);
    assert!(selected.status.success());
    assert_eq!(selected.stdout, unfiltered.stdout);
    let missing = run(&["--subpage", "1"], input);
    assert!(!missing.status.success());
    assert!(missing.stdout.is_empty());
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

#[test]
fn sniffing_recovers_truncated_captures_and_ignores_embedded_tti_records() {
    let mut header = vec![0x02, 0x15, 0x15, 0x15, 0x15, 0x15, 0x15, 0x15, 0x15, 0x15];
    header.extend([b' '; 32]);
    for tail in 1..42 {
        let mut input = header.clone();
        input.extend(vec![0xff; tail]);
        let detected = run(&[], &input);
        let explicit = run(&["--format", "t42"], &input);
        assert!(detected.status.success(), "tail {tail}");
        assert_eq!(detected.stdout, explicit.stdout);
    }
    let mut input = header;
    input.extend([0xff; 42]);
    input.extend(b"\nPN,20000\nOL,1,FALSE POSITIVE\n");
    let detected = run(&[], &input);
    let explicit = run(&["--format", "t42"], &input);
    assert!(detected.status.success());
    assert_eq!(detected.stdout, explicit.stdout);

    let tti = b"DE,Description\r\nXX,Retained record\r\nPN,10000\r\nOL,1,HELLO\r\n";
    assert_eq!(run(&[], tti).stdout, run(&["--format", "tti"], tti).stdout);
    for length in [960, 1000] {
        let raw = vec![b' '; length];
        assert_eq!(
            run(&[], &raw).stdout,
            run(&["--format", "raw"], &raw).stdout
        );
    }
}

#[test]
fn version_reports_the_package_version_without_reading_input() {
    for flag in ["--version", "-V"] {
        let result = run(&[flag], b"");
        assert!(result.status.success());
        assert_eq!(
            result.stdout,
            format!("ttx42 {}\n", env!("CARGO_PKG_VERSION")).as_bytes()
        );
    }
}

#[test]
fn hexadecimal_errors_identify_the_selector_and_value() {
    for flag in ["--page", "--subpage"] {
        for value in ["nope", "10000"] {
            let result = run(&[flag, value], b"");
            assert!(!result.status.success());
            let error = String::from_utf8_lossy(&result.stderr);
            assert!(error.contains(flag) && error.contains(value), "{error}");
        }
    }
}
