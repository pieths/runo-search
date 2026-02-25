# runo-search

Native Node.js addon in Rust (via napi-rs) for fast, regex-based file searching.

Uses `regex::bytes::Regex` with memory-mapped files (`memmap2`) to search files
using pre-compiled regex pattern strings, with AND semantics across multiple
patterns.

## Quick Start

```powershell
# Full bootstrap: downloads Node.js + Rust locally, installs deps, builds addon
.\build.ps1
```

That's it. Everything is installed locally — no system-wide tools needed (beyond
MSVC Build Tools which are required for the Rust linker).

## API

```typescript
export function searchFile(
    filePath: string,
    patterns: Array<string>,
    unicode: boolean,
    includeLines: boolean,
): Array<{ line: number; text: string }>;
```

- **filePath**: Absolute path to the file to search
- **patterns**: Array of regex pattern strings (AND semantics — all must match)
- **unicode**: `false` for raw byte mode (fast), `true` for Unicode-aware matching
- **includeLines**: `true` to include line text, `false` for line numbers only

Returns an empty array on no matches, errors, or invalid patterns (never throws).

## Prerequisites

- **MSVC C++ Build Tools** (Visual Studio or VS Build Tools)
- Everything else is downloaded automatically by `build.ps1`
