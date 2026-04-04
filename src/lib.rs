// Copyright (c) 2026 Piet Hein Schouten
// SPDX-License-Identifier: MIT

use std::cell::RefCell;

use memchr::memchr_iter;
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

#[napi(object)]
pub struct PatternMatch {
    /// Index into the input patterns array (0-based)
    pub pattern_index: u32,
    /// Number of matches for this pattern in the file
    pub frequency: u32,
    /// 1-based line numbers where this pattern matched (deduplicated, sorted)
    pub line_numbers: Vec<u32>,
}

#[napi(object)]
pub struct FilePatternMatches {
    /// Absolute file path
    pub file_path: String,
    /// Total number of lines in the file
    pub total_lines: u32,
    /// Per-pattern match data. Only patterns with >= 1 match are included.
    pub patterns: Vec<PatternMatch>,
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
/// - `case_insensitive`: If true, matching is case-insensitive.
///
/// Returns a single-element array with match data, or an empty array on
/// no match / error.
#[napi]
pub fn search_file_and(
    file_path: String,
    patterns: Vec<String>,
    unicode: bool,
    case_insensitive: bool,
) -> Vec<FilePatternMatches> {
    if patterns.is_empty() {
        return Vec::new();
    }

    // Build cache key from patterns + unicode flag.
    let mut cache_key = patterns.join("\0");
    cache_key.push('\0');
    cache_key.push(if unicode { '1' } else { '0' });
    cache_key.push(if case_insensitive { '1' } else { '0' });

    // Get or compile regexes (thread-local cache)
    CACHED.with(|cell| {
        let mut cache = cell.borrow_mut();

        let regexes = match &*cache {
            Some(cached) if cached.cache_key == cache_key => &cached.regexes,
            _ => {
                let new_regexes: Result<Vec<Regex>, _> = patterns
                    .iter()
                    .map(|pattern| {
                        regex::bytes::RegexBuilder::new(pattern)
                            .case_insensitive(case_insensitive)
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

        // Open and mmap the file
        let file = match std::fs::File::open(&file_path) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };

        let mmap = match unsafe { memmap2::Mmap::map(&file) } {
            Ok(m) => m,
            Err(_) => return Vec::new(),
        };

        let bytes = &mmap[..];
        let mut pattern_matches = Vec::new();

        for (idx, regex) in regexes.iter().enumerate() {
            let match_positions: Vec<usize> =
                regex.find_iter(bytes).map(|m| m.start()).collect();

            if match_positions.is_empty() {
                return Vec::new(); // AND failed — early exit
            }

            let frequency = match_positions.len() as u32;
            let line_numbers = positions_to_line_numbers(bytes, &match_positions);

            pattern_matches.push(PatternMatch {
                pattern_index: idx as u32,
                frequency,
                line_numbers,
            });
        }

        let total_lines = memchr_iter(b'\n', bytes).count() as u32 + 1;

        vec![FilePatternMatches {
            file_path,
            total_lines,
            patterns: pattern_matches,
        }]
    })
}

/// Search multiple files for matches using AND semantics across regex patterns.
/// All patterns must match somewhere in a file for that file's results to be returned.
/// Only files with one or more matches are included in the output.
///
/// - `file_paths`: Array of absolute file paths to search
/// - `patterns`: Array of regex pattern strings (AND semantics)
/// - `unicode`: If true, `.` matches full Unicode characters and `\w`/`\d`/`\s`
///   use Unicode classes. If false, raw byte mode for maximum performance.
/// - `case_insensitive`: If true, matching is case-insensitive.
///
/// Returns an array of `FilePatternMatches` for files where all patterns matched,
/// or an empty array on no match / error.
#[napi]
pub fn search_files_and(
    file_paths: Vec<String>,
    patterns: Vec<String>,
    unicode: bool,
    case_insensitive: bool,
) -> Vec<FilePatternMatches> {
    if patterns.is_empty() || file_paths.is_empty() {
        return Vec::new();
    }

    // Compile regexes once for the entire batch
    let regexes: Result<Vec<Regex>, _> = patterns
        .iter()
        .map(|pattern| {
            regex::bytes::RegexBuilder::new(pattern)
                .case_insensitive(case_insensitive)
                .multi_line(true)
                .unicode(unicode)
                .build()
        })
        .collect();

    let regexes = match regexes {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    let mut results = Vec::new();

    for file_path in &file_paths {
        let file = match std::fs::File::open(file_path) {
            Ok(f) => f,
            Err(_) => continue,
        };

        let mmap = match unsafe { memmap2::Mmap::map(&file) } {
            Ok(m) => m,
            Err(_) => continue,
        };

        let bytes = &mmap[..];
        let mut pattern_matches = Vec::new();
        let mut all_matched = true;

        for (idx, regex) in regexes.iter().enumerate() {
            let match_positions: Vec<usize> =
                regex.find_iter(bytes).map(|m| m.start()).collect();

            if match_positions.is_empty() {
                all_matched = false;
                break; // AND failed — early exit
            }

            let frequency = match_positions.len() as u32;
            let line_numbers = positions_to_line_numbers(bytes, &match_positions);

            pattern_matches.push(PatternMatch {
                pattern_index: idx as u32,
                frequency,
                line_numbers,
            });
        }

        if all_matched {
            let total_lines = memchr_iter(b'\n', bytes).count() as u32 + 1;
            results.push(FilePatternMatches {
                file_path: file_path.clone(),
                total_lines,
                patterns: pattern_matches,
            });
        }
    }

    results
}

/// Search multiple files for matches using OR semantics across regex patterns.
/// Each pattern is evaluated independently per file. Returns per-pattern
/// frequency and line number data.
///
/// - `file_paths`: Array of absolute file paths to search
/// - `patterns`: Array of regex pattern strings (each searched independently)
/// - `unicode`: If true, use Unicode character classes. False for performance.
/// - `case_insensitive`: If true, matching is case-insensitive.
///
/// Returns an array of `FilePatternMatches` for files with at least one pattern match.
#[napi]
pub fn search_files_or(
    file_paths: Vec<String>,
    patterns: Vec<String>,
    unicode: bool,
    case_insensitive: bool,
) -> Vec<FilePatternMatches> {
    if patterns.is_empty() || file_paths.is_empty() {
        return Vec::new();
    }

    // Compile regexes once for the entire batch
    let regexes: Result<Vec<Regex>, _> = patterns
        .iter()
        .map(|pattern| {
            regex::bytes::RegexBuilder::new(pattern)
                .case_insensitive(case_insensitive)
                .multi_line(true)
                .unicode(unicode)
                .build()
        })
        .collect();

    let regexes = match regexes {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    let mut results = Vec::new();

    for file_path in &file_paths {
        let file = match std::fs::File::open(file_path) {
            Ok(f) => f,
            Err(_) => continue,
        };

        let mmap = match unsafe { memmap2::Mmap::map(&file) } {
            Ok(m) => m,
            Err(_) => continue,
        };

        let bytes = &mmap[..];

        // Count total lines: number of newlines + 1
        let total_lines = memchr_iter(b'\n', bytes).count() as u32 + 1;

        let mut pattern_matches = Vec::new();

        for (idx, regex) in regexes.iter().enumerate() {
            let match_positions: Vec<usize> = regex.find_iter(bytes).map(|m| m.start()).collect();

            if match_positions.is_empty() {
                continue;
            }

            let frequency = match_positions.len() as u32;
            let line_numbers = positions_to_line_numbers(bytes, &match_positions);

            pattern_matches.push(PatternMatch {
                pattern_index: idx as u32,
                frequency,
                line_numbers,
            });
        }

        if !pattern_matches.is_empty() {
            results.push(FilePatternMatches {
                file_path: file_path.clone(),
                total_lines,
                patterns: pattern_matches,
            });
        }
    }

    results
}

// ============================================================================
// Line number calculation
// ============================================================================

/// Convert byte positions to deduplicated, sorted 1-based line numbers.
fn positions_to_line_numbers(bytes: &[u8], positions: &[usize]) -> Vec<u32> {
    let mut sorted_positions = positions.to_vec();
    sorted_positions.sort_unstable();

    let mut line_numbers = Vec::new();
    let mut current_line: u32 = 1;
    let mut last_pos: usize = 0;

    for &pos in &sorted_positions {
        current_line += memchr_iter(b'\n', &bytes[last_pos..pos]).count() as u32;
        last_pos = pos;

        if line_numbers.last() != Some(&current_line) {
            line_numbers.push(current_line);
        }
    }

    line_numbers
}
