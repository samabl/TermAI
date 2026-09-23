//! Test 11: drive the in-repo corpus (.trec) through the terminal.

use std::path::{Path, PathBuf};

use termai_vt::{parse_trec, run, Terminal};

fn corpus_files() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("corpus directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().map(|ext| ext == "txt").unwrap_or(false))
        .collect();
    files.sort();
    files
}

#[test]
fn corpus_is_present_and_large_enough() {
    let files = corpus_files();
    assert!(
        files.len() >= 6,
        "expected at least 6 corpus files, got {}",
        files.len()
    );
    let names: Vec<String> = files
        .iter()
        .map(|path| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert!(names.iter().filter(|name| name.contains("shell")).count() >= 2);
    assert!(names.iter().any(|name| name.contains("unterminated")));
}

#[test]
fn every_corpus_file_replays_green() {
    let files = corpus_files();
    assert!(!files.is_empty());
    for path in files {
        let text = std::fs::read_to_string(&path).expect("read corpus file");
        let script = parse_trec(&text).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
        let mut terminal = Terminal::new(80, 24);
        let report = run(&script, &mut terminal);
        assert!(
            report.failed.is_empty(),
            "{} failed: {:?}",
            path.display(),
            report.failed
        );
        assert!(report.passed > 0, "{} executed no steps", path.display());
    }
}
