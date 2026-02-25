// Copyright (c) 2026 Piet Hein Schouten
// SPDX-License-Identifier: MIT

use std::cell::RefCell;

use memchr::{memchr, memrchr, memchr_iter};
use napi_derive::napi;
use regex::bytes::Regex;

// ============================================================================
// Types
// ============================================================================

struct CachedSearch {
    /// Cache key: the pattern strings joined with a \0 delimiter,
    /// plus \0 + "1" or "0" for the unicode flag.
    cache_key: String,
    regexes: Vec<Regex>,
}

struct LineResult {
    line: u32,
    text: String,
}

#[napi(object)]
pub struct SearchLineResult {
    pub line: u32,
    pub text: String,
}

// ============================================================================
// Thread-local regex cache
// ============================================================================

thread_local! {
    static CACHED: RefCell<Option<CachedSearch>> = RefCell::new(None);
}

// ============================================================================
// Exported napi function
// ============================================================================

/// Search a file for matches using AND semantics across regex patterns.
/// All patterns must match somewhere in the file for results to be returned.
///
/// - `file_path`: Absolute file path to search
/// - `patterns`: Array of regex pattern strings (AND semantics)
/// - `unicode`: If true, `.` matches full Unicode characters and `\w`/`\d`/`\s`
///   use Unicode classes. If false, raw byte mode for maximum performance.
/// - `include_lines`: If true, each result includes the full line text.
///   If false, the `text` field is set to an empty string.
///
/// Returns an array of `{line, text}` results, or an empty array on no match / error.
#[napi]
pub fn search_file(
    file_path: String,
    patterns: Vec<String>,
    unicode: bool,
    include_lines: bool,
) -> Vec<SearchLineResult> {
    if patterns.is_empty() {
        return Vec::new();
    }

    // 1. Build cache key from patterns + unicode flag.
    //    include_lines is NOT part of the cache key — it doesn't affect
    //    regex compilation, only output formatting.
    let mut cache_key = patterns.join("\0");
    cache_key.push('\0');
    cache_key.push(if unicode { '1' } else { '0' });

    // 2. Get or compile regexes (thread-local cache)
    CACHED.with(|cell| {
        let mut cache = cell.borrow_mut();

        let regexes = match &*cache {
            Some(cached) if cached.cache_key == cache_key => &cached.regexes,
            _ => {
                let new_regexes: Result<Vec<Regex>, _> = patterns
                    .iter()
                    .map(|pattern| {
                        regex::bytes::RegexBuilder::new(pattern)
                            .case_insensitive(true)
                            .multi_line(true)
                            .unicode(unicode)
                            .build()
                    })
                    .collect();

                // If any pattern fails to compile, return empty results
                let new_regexes = match new_regexes {
                    Ok(r) => r,
                    Err(_) => return Vec::new(),
                };

                *cache = Some(CachedSearch {
                    cache_key,
                    regexes: new_regexes,
                });
                &cache.as_ref().unwrap().regexes
            }
        };

        // 3. Open and mmap the file
        let file = match std::fs::File::open(&file_path) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };

        let mmap = match unsafe { memmap2::Mmap::map(&file) } {
            Ok(m) => m,
            Err(_) => return Vec::new(),
        };

        // 4. Search
        let results = search_file_impl(&mmap, regexes, include_lines);

        // 5. Convert to napi return type
        results
            .into_iter()
            .map(|r| SearchLineResult {
                line: r.line,
                text: r.text,
            })
            .collect()
    })
}

// ============================================================================
// Core search logic
// ============================================================================

/// Sequential regex matching with AND semantics and early exit.
fn search_file_impl(bytes: &[u8], regexes: &[Regex], include_lines: bool) -> Vec<LineResult> {
    let mut all_match_positions: Vec<usize> = Vec::new();

    for regex in regexes {
        let matches: Vec<usize> = regex.find_iter(bytes).map(|m| m.start()).collect();

        if matches.is_empty() {
            return Vec::new(); // AND failed — early exit
        }

        all_match_positions.extend(matches);
    }

    // Convert byte positions to line numbers + optionally extract line text
    // Deduplicate by line number, sort by line number
    positions_to_line_results(bytes, &mut all_match_positions, include_lines)
}

// ============================================================================
// Line number calculation and text extraction
// ============================================================================

fn positions_to_line_results(
    bytes: &[u8],
    positions: &mut Vec<usize>,
    include_lines: bool,
) -> Vec<LineResult> {
    // Sort positions so we can do a single forward pass for line counting
    positions.sort_unstable();
    positions.dedup();

    let mut results = Vec::new();
    let mut seen_lines = std::collections::HashSet::new();
    let mut current_line: u32 = 1;
    let mut last_pos: usize = 0;

    for &pos in positions.iter() {
        // Count newlines from last_pos to pos (progressive line counting)
        current_line += memchr_iter(b'\n', &bytes[last_pos..pos]).count() as u32;
        last_pos = pos;

        if seen_lines.insert(current_line) {
            let text = if include_lines {
                extract_line_text(bytes, pos)
            } else {
                String::new()
            };
            results.push(LineResult {
                line: current_line,
                text,
            });
        }
    }

    results
}

fn extract_line_text(bytes: &[u8], pos: usize) -> String {
    // Find line start (after previous \n, or start of file)
    let line_start = match memrchr(b'\n', &bytes[..pos]) {
        Some(i) => i + 1,
        None => 0,
    };

    // Find line end (next \n, or end of file)
    let line_end = match memchr(b'\n', &bytes[pos..]) {
        Some(i) => pos + i,
        None => bytes.len(),
    };

    // Extract line, strip trailing \r (Windows line endings)
    let line_bytes = &bytes[line_start..line_end];
    let text = String::from_utf8_lossy(line_bytes);
    let text = text.trim_end_matches(|c| c == '\r' || c == '\n');
    text.to_string()
}
