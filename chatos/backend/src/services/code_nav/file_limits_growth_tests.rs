// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    open_code_nav_file, read_code_nav_line_preview, read_code_nav_line_preview_from_reader,
    CODE_NAV_MAX_FILE_BYTES,
};
use std::fs;
use std::io::{self, Cursor, Read};
use std::path::{Path, PathBuf};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("code-nav-growth-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(fs::canonicalize(path).unwrap())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn line_preview_rejects_file_growth_after_open_size_check() {
    let fixture = Fixture::new();
    let path = fixture.0.join("source.rs");
    let limit = CODE_NAV_MAX_FILE_BYTES as usize;
    // Exercise both an oversized first line and the cumulative scan budget.
    for (prefix, requested_line) in [(Vec::new(), 1), (b"x\n".repeat(limit / 2), limit / 2 + 1)] {
        fs::write(&path, "small").unwrap();
        let opened = open_code_nav_file(&path).unwrap();
        let mut grown = prefix;
        grown.extend(vec![b'x'; limit + 32]);
        // Mutate the same file after the production opener accepted its size.
        // No concurrent writer or timing-dependent race is required.
        fs::write(&path, &grown).unwrap();
        let err = read_code_nav_line_preview_from_reader(&path, opened, requested_line, 4)
            .expect_err("growth must not bypass the preview read budget");
        assert!(err.contains("code-nav file exceeds limit"));
        assert_eq!(fs::read(&path).unwrap(), grown);
    }
}

struct CountingReader {
    remaining: u64,
    read: u64,
}

impl Read for CountingReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = (buffer.len() as u64).min(self.remaining) as usize;
        buffer[..count].fill(b'x');
        self.remaining -= count as u64;
        self.read += count as u64;
        Ok(count)
    }
}

#[test]
fn line_preview_reads_at_most_limit_plus_one_from_source() {
    let mut source = CountingReader {
        remaining: CODE_NAV_MAX_FILE_BYTES * 2,
        read: 0,
    };
    let result = read_code_nav_line_preview_from_reader(Path::new("source.rs"), &mut source, 1, 4);
    assert!(
        source.read <= CODE_NAV_MAX_FILE_BYTES + 1,
        "underlying reader consumed {} bytes",
        source.read
    );
    assert!(result.unwrap_err().contains("code-nav file exceeds limit"));
}

#[test]
fn line_preview_preserves_boundaries_text_and_early_return() {
    let fixture = Fixture::new();
    let path = fixture.0.join("source.rs");
    let limit = CODE_NAV_MAX_FILE_BYTES as usize;
    for (content, line, max_chars, expected) in [
        (Vec::new(), 1, 4, ""),
        (vec![b'x'; limit], 1, 4, "xxxx"),
        (vec![b'x'; limit], 2, 4, ""),
        ([vec![b'x'; limit - 1], vec![b'\n']].concat(), 2, 4, ""),
        (b"first\r\nsecond\r\n".to_vec(), 2, 4, "seco"),
        ("first\n你好世界\n".as_bytes().to_vec(), 2, 2, "你好"),
        (vec![0xff, b'x', b'\n'], 1, 2, "�x"),
        (b"text".to_vec(), 1, 0, ""),
    ] {
        fs::write(&path, content).unwrap();
        assert_eq!(
            read_code_nav_line_preview(&path, line, max_chars).unwrap(),
            expected
        );
    }
    assert_eq!(
        read_code_nav_line_preview(&fixture.0.join("missing"), 0, 4).unwrap(),
        ""
    );

    let mut source = Cursor::new(b"first\n".to_vec()).chain(CountingReader {
        remaining: CODE_NAV_MAX_FILE_BYTES * 2,
        read: 0,
    });
    assert_eq!(
        read_code_nav_line_preview_from_reader(&path, &mut source, 1, 4).unwrap(),
        "firs"
    );
    assert_eq!(
        source.get_ref().1.read,
        0,
        "do not scan past requested line"
    );
}

#[test]
fn line_preview_propagates_read_errors() {
    struct BrokenReader;
    impl Read for BrokenReader {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("test read failure"))
        }
    }
    let source = Cursor::new(b"first\n".to_vec()).chain(BrokenReader);
    let error =
        read_code_nav_line_preview_from_reader(Path::new("source.rs"), source, 2, 4).unwrap_err();
    assert!(error.contains("read code-nav line failed: test read failure"));
}
