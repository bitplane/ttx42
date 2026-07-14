use std::{fs, path::Path};

use ttx42::{AnsiOptions, DecodeOptions, Page, decode, to_ansi};

#[test]
#[ignore = "requires corpus/fetch.sh"]
fn corpus_decodes_and_renders_without_panics() {
    let root = Path::new("corpus");
    let mut files = 0;
    let mut pages = 0;
    visit(root, &mut |path| {
        if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("t42"))
        {
            files += 1;
            let bytes = fs::read(path).unwrap();
            for page in Page::parse_t42(&bytes).unwrap() {
                pages += 1;
                let grid = decode(&page, &DecodeOptions::default());
                assert!(!to_ansi(&grid, &AnsiOptions::default()).is_empty());
            }
        }
    });
    assert!(files > 0, "run corpus/fetch.sh first");
    assert!(pages > 0, "corpus contained no decodable pages");
}

fn visit(path: &Path, callback: &mut impl FnMut(&Path)) {
    if path.is_file() {
        callback(path);
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        visit(&entry.path(), callback);
    }
}
