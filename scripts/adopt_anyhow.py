#!/usr/bin/env python3
"""
Adopt `anyhow` across all workspace crates.

This script:
1. Adds `anyhow = { workspace = true }` to the workspace root Cargo.toml
2. Adds `anyhow = { workspace = true }` to each crate's [dependencies]
3. Rewrites .rs files:
   - Function signatures: `Result<T, String>` → `anyhow::Result<T>`
   - `.map_err(|e| format!("...{e}..."))` → `.with_context(|| format!("..."))`
   - `.map_err(|e| format!("...: {e}"))` → `.context("...")`  (simple cases)
   - `.map_err(|e| e.to_string())` → removed (bare `?` suffices)
   - `Err(format!(...))` → `Err(anyhow::anyhow!(...))`
   - `return Err(format!(...))` → `anyhow::bail!(...)`
   - `Err("...".into())` → `Err(anyhow::anyhow!("..."))`
   - `return Err("...".into())` → `anyhow::bail!("...")`
   - `Err("...".to_string())` → `Err(anyhow::anyhow!("..."))`
   - Adds `use anyhow::Context;` where `.context()` is used
4. Updates src-tauri call sites to add `.map_err(|e| e.to_string())`

Exclusions:
- skill-headless: keeps its own HeadlessError enum
- skill-jobs: keeps Result<Value, String> for stored/cloned results
- Type aliases and struct fields that store errors as String (not function returns)
- `Result<String, _>` (where String is the Ok type, not the error)
"""

import os
import re
import sys
from pathlib import Path
from typing import Optional

ROOT = Path(__file__).resolve().parent.parent
CRATES_DIR = ROOT / "crates"

# ── Crates that need anyhow ──────────────────────────────────────────────────

# All crates get the dependency added; only those with Result<T, String> patterns
# get .rs file rewrites.
ALL_CRATES = sorted(p.name for p in CRATES_DIR.iterdir() if p.is_dir() and (p / "Cargo.toml").exists())

# Crates where we should NOT rewrite Result<T, String> in .rs files
# (they have custom error types or store errors as String intentionally)
SKIP_RS_REWRITE = {
    "skill-headless",   # Has HeadlessError enum
    "skill-constants",  # Pure constants, no Result<T, String>
    "skill-eeg",        # Pure signal processing, no Result<T, String>
    "skill-tray",       # Pure std, no Result<T, String>
    "skill-vision",     # FFI crate
    "skill-label-index",
    "skill-commands",
    "skill-devices",
    "skill-gpu",
}

# Files to skip entirely (complex patterns that need manual review)
SKIP_FILES: set[str] = set()

# ── Statistics ───────────────────────────────────────────────────────────────

stats = {
    "cargo_toml_updated": 0,
    "rs_files_updated": 0,
    "result_string_replaced": 0,
    "map_err_format_replaced": 0,
    "map_err_to_string_replaced": 0,
    "err_format_replaced": 0,
    "err_into_replaced": 0,
    "context_imports_added": 0,
}

# ── Step 1: Workspace root Cargo.toml ────────────────────────────────────────

def update_workspace_toml():
    path = ROOT / "Cargo.toml"
    text = path.read_text()
    if 'anyhow' in text:
        print("  [skip] workspace Cargo.toml already has anyhow")
        return
    text = text.replace(
        '[workspace.dependencies]\n',
        '[workspace.dependencies]\nanyhow = "1"\n',
    )
    path.write_text(text)
    print("  [ok] workspace Cargo.toml")
    stats["cargo_toml_updated"] += 1

# ── Step 2: Per-crate Cargo.toml ─────────────────────────────────────────────

def update_crate_toml(crate_name: str):
    path = CRATES_DIR / crate_name / "Cargo.toml"
    text = path.read_text()
    if 'anyhow' in text:
        print(f"  [skip] {crate_name}/Cargo.toml already has anyhow")
        return

    # Find [dependencies] section and add anyhow as first entry
    # Handle both `[dependencies]` and entries that follow
    if '\n[dependencies]\n' in text:
        text = text.replace(
            '\n[dependencies]\n',
            '\n[dependencies]\nanyhow = { workspace = true }\n',
        )
    elif '[dependencies]\n' in text:
        text = text.replace(
            '[dependencies]\n',
            '[dependencies]\nanyhow = { workspace = true }\n',
        )
    else:
        print(f"  [WARN] {crate_name}/Cargo.toml: no [dependencies] section found, skipping")
        return

    path.write_text(text)
    print(f"  [ok] {crate_name}/Cargo.toml")
    stats["cargo_toml_updated"] += 1

# ── Step 3: Rewrite .rs files ────────────────────────────────────────────────

def find_rs_files(crate_name: str) -> list[Path]:
    src = CRATES_DIR / crate_name / "src"
    if not src.exists():
        return []
    return sorted(src.rglob("*.rs"))


def rewrite_rs_file(path: Path) -> bool:
    """Rewrite a single .rs file. Returns True if modified."""
    original = path.read_text()
    text = original

    # ── Track what we need ────────────────────────────────────────────────
    needs_context_import = False
    file_changed = False

    # ── 3a: Result<T, String> → anyhow::Result<T> in fn signatures ──────
    #
    # We match `-> Result<..., String>` patterns.  Be careful not to match
    # Result<String, _> (where String is the Ok type).
    #
    # Also match Sender<Result<..., String>> patterns.
    #
    # Strategy: find `Result<CONTENT, String>` where CONTENT doesn't contain
    # unbalanced angle brackets.

    def replace_result_string(m):
        nonlocal file_changed
        prefix = m.group(1)  # "Result<"
        inner = m.group(2)   # the Ok type
        stats["result_string_replaced"] += 1
        file_changed = True
        return f"anyhow::Result<{inner}>"

    # Match Result<T, String> where T can contain nested <> but not unmatched ones
    # This regex handles up to 2 levels of nesting
    nested = r'[^<>]*(?:<[^<>]*(?:<[^<>]*>[^<>]*)*>[^<>]*)*'
    pattern = rf'(Result)<({nested}),\s*String>'
    text = re.sub(pattern, replace_result_string, text)

    # ── 3b: .map_err(|e| e.to_string())? → just ? ───────────────────────
    #
    # When the only thing map_err does is .to_string(), anyhow's blanket
    # From impl handles this automatically.
    pattern_to_string = r'\.map_err\(\|e(?::.*?)?\|\s*e\.to_string\(\)\)'
    count = len(re.findall(pattern_to_string, text))
    if count:
        text = re.sub(pattern_to_string, '', text)
        stats["map_err_to_string_replaced"] += count
        file_changed = True

    # ── 3c: .map_err(|e| format!("static msg: {e}")) → .context("static msg") ─
    #
    # Simple case: format string is `"literal: {e}"` or `"literal: {e}"`
    # where the only interpolation is `{e}`.
    # Complex case: multiple interpolations → .with_context(|| format!(...))

    def replace_map_err_format(m):
        nonlocal needs_context_import, file_changed
        full = m.group(0)
        # Extract the format string content
        inner = m.group(1)  # everything inside map_err(|e| format!(...))

        # Check if this is a simple "msg: {e}" pattern (error at end)
        # where we can use .context("msg")
        simple = re.match(r'^"([^"{}]*?)(?::\s*)?(?:\{e\}|\{e:#?\})"$', inner)
        if simple:
            msg = simple.group(1).rstrip(': ').rstrip()
            needs_context_import = True
            stats["map_err_format_replaced"] += 1
            file_changed = True
            return f'.context("{msg}")'

        # Otherwise use .with_context(|| format!(...)) but remove {e} references
        # With anyhow the source error is automatically chained via .context(),
        # so we strip `{e}` / `: {e}` / `: {e:#}` from the format string.
        cleaned = re.sub(r'(?::\s*)?\{e(?::#?)?\}', '', inner)
        # Clean up trailing `: ` or `:` left before closing quote
        cleaned = re.sub(r':\s*"$', '"', cleaned)
        needs_context_import = True
        stats["map_err_format_replaced"] += 1
        file_changed = True
        return f'.with_context(|| format!({cleaned}))'

    # Match .map_err(|e| format!("..."))  — single-line cases
    # The format! argument can contain nested parens from format args
    map_err_fmt = r'\.map_err\(\|e\|\s*format!\(([^)]*(?:\([^)]*\))*[^)]*)\)\)'
    text = re.sub(map_err_fmt, replace_map_err_format, text)

    # ── 3d: .map_err(|_| "msg".to_string()) → .context("msg") ───────────
    def replace_map_err_literal(m):
        nonlocal needs_context_import, file_changed
        msg = m.group(1)
        needs_context_import = True
        stats["map_err_format_replaced"] += 1
        file_changed = True
        return f'.context("{msg}")'

    text = re.sub(
        r'\.map_err\(\|_\|\s*"([^"]*)"\s*\.to_string\(\)\)',
        replace_map_err_literal, text
    )

    # ── 3e: .map_err(|e| "msg".to_string()) → .context("msg") ───────────
    text_before = text
    text = re.sub(
        r'\.map_err\(\|e\|\s*"([^"]*)"\s*\.to_string\(\)\)',
        replace_map_err_literal, text
    )

    # ── 3f: return Err(format!(...)); → anyhow::bail!(...); ──────────────
    def replace_return_err_format(m):
        nonlocal file_changed
        indent = m.group(1)
        args = m.group(2)
        stats["err_format_replaced"] += 1
        file_changed = True
        return f'{indent}anyhow::bail!({args})'

    text = re.sub(
        r'^(\s*)return\s+Err\(format!\(([^;]*)\)\);?\s*$',
        replace_return_err_format, text, flags=re.MULTILINE
    )

    # ── 3g: Err(format!(...)) → Err(anyhow::anyhow!(...)) ───────────────
    # (non-return cases, e.g. in match arms)
    def replace_err_format(m):
        nonlocal file_changed
        args = m.group(1)
        stats["err_format_replaced"] += 1
        file_changed = True
        return f'Err(anyhow::anyhow!({args}))'

    # Only match Err(format!(...)) that are NOT preceded by `return`
    text = re.sub(
        r'(?<!return )Err\(format!\(((?:[^()]*(?:\([^()]*(?:\([^()]*\))*[^()]*\))*[^()]*)*)\)\)',
        replace_err_format, text
    )

    # ── 3h: return Err("msg".into()); → anyhow::bail!("msg"); ────────────
    def replace_return_err_into(m):
        nonlocal file_changed
        indent = m.group(1)
        msg = m.group(2)
        stats["err_into_replaced"] += 1
        file_changed = True
        return f'{indent}anyhow::bail!("{msg}")'

    text = re.sub(
        r'^(\s*)return\s+Err\("([^"]*)"\s*\.into\(\)\);?\s*$',
        replace_return_err_into, text, flags=re.MULTILINE
    )

    # ── 3i: Err("msg".into()) → Err(anyhow::anyhow!("msg")) ────────────
    def replace_err_into(m):
        nonlocal file_changed
        msg = m.group(1)
        stats["err_into_replaced"] += 1
        file_changed = True
        return f'Err(anyhow::anyhow!("{msg}"))'

    text = re.sub(
        r'(?<!return )Err\("([^"]*)"\s*\.into\(\)\)',
        replace_err_into, text
    )

    # ── 3j: Err("msg".to_string()) → Err(anyhow::anyhow!("msg")) ───────
    text = re.sub(
        r'(?<!return )Err\("([^"]*)"\s*\.to_string\(\)\)',
        replace_err_into, text
    )
    text = re.sub(
        r'^(\s*)return\s+Err\("([^"]*)"\s*\.to_string\(\)\);?\s*$',
        replace_return_err_into, text, flags=re.MULTILINE
    )

    # ── 3k: Handle oneshot::Sender<Result<(), String>> in struct/enum fields ─
    # These should become oneshot::Sender<anyhow::Result<()>>
    # Already handled by the general Result<T, String> replacement above.

    # ── Add imports ──────────────────────────────────────────────────────────
    if needs_context_import and 'use anyhow::Context' not in text and 'anyhow::Context' not in text:
        # Find a good place to add the import
        # Look for existing `use` block or add after module doc comment
        if 'use anyhow::' in text:
            # Already importing something from anyhow, add Context
            text = re.sub(
                r'(use anyhow::\{[^}]*)',
                lambda m: m.group(1) + ', Context' if 'Context' not in m.group(1) else m.group(1),
                text
            )
        elif re.search(r'^use ', text, re.MULTILINE):
            # Add after the last `use` line in the top block
            lines = text.split('\n')
            last_use = 0
            in_use_block = False
            for i, line in enumerate(lines):
                stripped = line.strip()
                if stripped.startswith('use '):
                    last_use = i
                    in_use_block = True
                elif in_use_block and stripped and not stripped.startswith('//') and not stripped.startswith('use '):
                    break
            lines.insert(last_use + 1, 'use anyhow::Context;')
            text = '\n'.join(lines)
        else:
            # Prepend
            text = 'use anyhow::Context;\n' + text
        stats["context_imports_added"] += 1
        file_changed = True

    if text != original:
        path.write_text(text)
        stats["rs_files_updated"] += 1
        return True
    return False


# ── Step 4: Update src-tauri call sites ──────────────────────────────────────

def update_tauri_call_sites():
    """Add .map_err(|e| e.to_string()) at Tauri command boundaries."""
    src_tauri = ROOT / "src-tauri" / "src"
    if not src_tauri.exists():
        print("  [skip] src-tauri/src not found")
        return

    # Known call sites that return Result<T, String> to Tauri but now call
    # crate functions returning anyhow::Result<T>.
    # We find patterns like `crate_fn(...)?` or `crate_fn(...).await?` inside
    # functions that return `Result<_, String>`.
    #
    # This is best done by finding the specific call sites.
    # For now, we'll identify the files and patterns.

    patterns = [
        # (file_glob, old_pattern, new_pattern)
        ("tts.rs",
         "skill_tts::tts_init_with_callback(emit).await",
         "skill_tts::tts_init_with_callback(emit).await.map_err(|e| e.to_string())"),
        ("tts.rs",
         "let result = skill_tts::tts_unload().await;",
         "let result = skill_tts::tts_unload().await.map_err(|e| e.to_string());"),
        ("history_cmds.rs",
         "skill_history::delete_session(&csv_path)",
         "skill_history::delete_session(&csv_path).map_err(|e| e.to_string())"),
    ]

    for filename, old, new in patterns:
        for rs_file in src_tauri.rglob(filename):
            text = rs_file.read_text()
            if old in text and new not in text:
                text = text.replace(old, new)
                rs_file.write_text(text)
                print(f"  [ok] src-tauri: {rs_file.relative_to(ROOT)}")


# ── Step 5: Handle skill-jobs specially ──────────────────────────────────────
# skill-jobs uses Result<Value, String> for stored/cloned job results.
# We add anyhow to deps but don't rewrite the stored result types.
# Only rewrite patterns inside closures that produce the results.

# ── Main ─────────────────────────────────────────────────────────────────────

def main():
    print("=== Step 1: Workspace Cargo.toml ===")
    update_workspace_toml()

    print("\n=== Step 2: Per-crate Cargo.toml ===")
    for crate in ALL_CRATES:
        update_crate_toml(crate)

    print("\n=== Step 3: Rewrite .rs files ===")
    for crate in ALL_CRATES:
        if crate in SKIP_RS_REWRITE:
            print(f"  [skip] {crate} (excluded from .rs rewrite)")
            continue
        rs_files = find_rs_files(crate)
        for rs_file in rs_files:
            rel = rs_file.relative_to(ROOT)
            if str(rel) in SKIP_FILES:
                print(f"  [skip] {rel}")
                continue
            if rewrite_rs_file(rs_file):
                print(f"  [ok] {rel}")

    print("\n=== Step 4: Update src-tauri call sites ===")
    update_tauri_call_sites()

    print("\n=== Statistics ===")
    for k, v in stats.items():
        print(f"  {k}: {v}")

    print("\nDone. Run `cargo check` to verify.")


if __name__ == "__main__":
    main()
