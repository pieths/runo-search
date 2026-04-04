// Copyright (c) 2026 Piet Hein Schouten
// SPDX-License-Identifier: MIT

// ============================================================================
// Simple test script for runo-search
//
// Usage:
//   node test.js <file-path> <pattern1> [pattern2] [pattern3] ...
//
// Examples:
//   node test.js "C:\projects\myfile.txt" "hello"
//   node test.js "./src/lib.rs" "fn\s+\w+" "impl"
//   node test.js "D:\logs\app.log" "error" "timeout"
//
// All patterns use AND semantics — every pattern must match somewhere
// in the file for any results to be returned.
// ============================================================================

const { searchFileAnd } = require("./index");

const args = process.argv.slice(2);

if (args.length < 2) {
    console.log("Usage: node test.js <file-path> <pattern1> [pattern2] ...");
    process.exit(1);
}

const filePath = args[0];
const patterns = args.slice(1);

console.log(`File:     ${filePath}`);
console.log(`Patterns: ${patterns.join(", ")}`);
console.log();

const results = searchFileAnd(filePath, patterns, /* unicode */ false, /* caseInsensitive */ false);

if (results.length === 0) {
    console.log("No matches found.");
} else {
    const file = results[0];
    console.log(`Total lines: ${file.totalLines}`);
    console.log(`Patterns matched: ${file.patterns.length}\n`);
    for (const p of file.patterns) {
        console.log(`  Pattern ${p.patternIndex}: ${p.frequency} match(es) on lines ${p.lineNumbers.join(", ")}`);
    }
}
