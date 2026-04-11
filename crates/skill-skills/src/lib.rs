// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! `skill-skills` — Agent Skills discovery, parsing, and prompt injection.
//!
//! Discovers `SKILL.md` files from multiple locations and makes them available
//! to the LLM chat so it can load specialised instructions on demand.
//!
//! ## Discovery locations (priority order)
//!
//! 1. **User-global**: `~/.skill/skills/`
//! 2. **Project-local**: `<cwd>/.skill/skills/`
//! 3. **Bundled / dev**: `<app_root>/skills/` (git submodule)
//! 4. **Explicit paths**: passed via `skill_paths`
//!
//! ## Discovery algorithm
//!
//! For each directory scanned:
//! - If the directory contains `SKILL.md`, treat it as a skill root (load it,
//!   do **not** recurse further).
//! - Otherwise, at the root level only, load any direct `.md` children as skills.
//! - Recurse into subdirectories (skipping `.`-prefixed dirs and `node_modules`)
//!   to find `SKILL.md` files deeper down.
//! - Respects `.gitignore`, `.ignore`, `.fdignore` for filtering.
//!
//! ## Skill file format
//!
//! Each `.md` file may have YAML frontmatter with:
//! - `name` — optional; defaults to parent directory name.
//! - `description` — **required** (max 1024 chars); skills without one are dropped.
//! - `disable-model-invocation` — if `true`, excluded from the system prompt.

#[cfg(feature = "sync")]
pub mod sync;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use ignore::gitignore::GitignoreBuilder;

// ── Constants ─────────────────────────────────────────────────────────────────

const MAX_NAME_LENGTH: usize = 64;
const MAX_DESCRIPTION_LENGTH: usize = 1024;
const SKILL_MARKER: &str = skill_constants::SKILL_MARKER;
const SKILLS_SUBDIR: &str = skill_constants::SKILLS_SUBDIR;

// ── Types ─────────────────────────────────────────────────────────────────────

/// A discovered and validated skill.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Skill {
    /// Unique kebab-case name.
    pub name: String,
    /// Human-readable description (from frontmatter).
    pub description: String,
    /// Absolute path to the skill `.md` file.
    pub file_path: String,
    /// Directory containing the skill file (for resolving relative paths).
    pub base_dir: String,
    /// Where the skill was found: `"user"`, `"project"`, `"bundled"`, or `"path"`.
    pub source: String,
    /// If true, the skill is excluded from the system prompt (only usable via
    /// explicit invocation).
    pub disable_model_invocation: bool,
}

/// A diagnostic message from skill loading.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SkillDiagnostic {
    pub level: String, // "warning" | "collision"
    pub message: String,
    pub path: String,
}

/// Result of loading skills.
#[derive(Debug, Clone, Default)]
pub struct LoadSkillsResult {
    pub skills: Vec<Skill>,
    pub diagnostics: Vec<SkillDiagnostic>,
}

/// Options for the top-level `load_skills` function.
pub struct LoadSkillsOptions {
    /// Working directory for project-local skills. Default: current dir.
    pub cwd: PathBuf,
    /// The `~/.skill` directory (user data dir).
    pub skill_dir: PathBuf,
    /// Path to the bundled/dev skills directory (e.g. `<app_root>/skills/`).
    pub bundled_dir: Option<PathBuf>,
    /// Explicit skill file/directory paths.
    pub skill_paths: Vec<PathBuf>,
    /// Whether to include the default directories. Default: true.
    pub include_defaults: bool,
}

impl Default for LoadSkillsOptions {
    fn default() -> Self {
        Self {
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            skill_dir: default_skill_dir(),
            bundled_dir: None,
            skill_paths: Vec::new(),
            include_defaults: true,
        }
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Load skills from all configured locations.
///
/// Returns skills and any validation diagnostics.  Skills are deduplicated by
/// name; the first loaded wins (user > project > bundled > explicit paths).
pub fn load_skills(options: &LoadSkillsOptions) -> LoadSkillsResult {
    let mut skill_map: HashMap<String, Skill> = HashMap::new();
    let mut real_paths: HashSet<PathBuf> = HashSet::new();
    let mut all_diags: Vec<SkillDiagnostic> = Vec::new();
    let mut collision_diags: Vec<SkillDiagnostic> = Vec::new();

    let mut add = |result: LoadSkillsResult| {
        all_diags.extend(result.diagnostics);
        for skill in result.skills {
            // Resolve symlinks to detect duplicate files.
            let real = fs::canonicalize(&skill.file_path).unwrap_or_else(|_| PathBuf::from(&skill.file_path));
            if real_paths.contains(&real) {
                continue; // silently skip symlink duplicates
            }

            if let Some(existing) = skill_map.get(&skill.name) {
                collision_diags.push(SkillDiagnostic {
                    level: "collision".into(),
                    message: format!(
                        "skill name \"{}\" collision: \"{}\" loses to \"{}\"",
                        skill.name, skill.file_path, existing.file_path
                    ),
                    path: skill.file_path.clone(),
                });
            } else {
                real_paths.insert(real);
                skill_map.insert(skill.name.clone(), skill);
            }
        }
    };

    if options.include_defaults {
        // 1. User-global: ~/.skill/skills/ AND ~/.skill/ root
        let user_skills_dir = options.skill_dir.join(SKILLS_SUBDIR);
        add(load_skills_from_dir(&user_skills_dir, "user", true));
        // Also scan skill_dir root itself so users can drop SKILL.md files
        // directly into ~/.skill/ (or a custom data dir) without the skills/ subdir.
        add(load_skills_from_dir(&options.skill_dir, "user", true));

        // 2. Project-local: <cwd>/.skill/skills/ AND <cwd>/.skill/ root
        let project_base = options.cwd.join(skill_constants::SKILL_DIR);
        let project_skills_dir = project_base.join(SKILLS_SUBDIR);
        add(load_skills_from_dir(&project_skills_dir, "project", true));
        add(load_skills_from_dir(&project_base, "project", true));

        // 3. Bundled / dev: <app_root>/skills/
        if let Some(ref bundled) = options.bundled_dir {
            add(load_skills_from_dir(bundled, "bundled", true));
        }
    }

    // 4. Explicit paths.
    // Collect results first to avoid borrow conflicts with the `add` closure.
    let mut explicit_results: Vec<LoadSkillsResult> = Vec::new();
    for raw_path in &options.skill_paths {
        let resolved = if raw_path.is_absolute() {
            raw_path.clone()
        } else {
            options.cwd.join(raw_path)
        };

        if !resolved.exists() {
            explicit_results.push(LoadSkillsResult {
                skills: Vec::new(),
                diagnostics: vec![SkillDiagnostic {
                    level: "warning".into(),
                    message: "skill path does not exist".into(),
                    path: resolved.display().to_string(),
                }],
            });
            continue;
        }

        if resolved.is_dir() {
            explicit_results.push(load_skills_from_dir(&resolved, "path", true));
        } else if resolved.is_file() && resolved.extension().map(|e| e == "md").unwrap_or(false) {
            let (skill, diags) = load_skill_from_file(&resolved, "path");
            let skills = skill.into_iter().collect();
            explicit_results.push(LoadSkillsResult {
                skills,
                diagnostics: diags,
            });
        } else {
            explicit_results.push(LoadSkillsResult {
                skills: Vec::new(),
                diagnostics: vec![SkillDiagnostic {
                    level: "warning".into(),
                    message: "skill path is not a directory or .md file".into(),
                    path: resolved.display().to_string(),
                }],
            });
        }
    }
    for result in explicit_results {
        add(result);
    }

    all_diags.extend(collision_diags);
    LoadSkillsResult {
        skills: skill_map.into_values().collect(),
        diagnostics: all_diags,
    }
}

/// Format discovered skills for inclusion in a system prompt.
///
/// Produces an XML block listing each skill's name, description, and file
/// location so the LLM can use the `read_file` tool to load full instructions
/// when the task matches.
///
/// Skills with `disable_model_invocation = true` are excluded.
pub fn format_skills_for_prompt(skills: &[Skill]) -> String {
    let visible: Vec<&Skill> = skills.iter().filter(|s| !s.disable_model_invocation).collect();
    if visible.is_empty() {
        return String::new();
    }

    let mut lines = vec![
        String::new(),
        String::new(),
        "The following skills provide specialised instructions for specific tasks.".into(),
        "Use the read_file tool to load a skill's file when the task matches its description.".into(),
        "When a skill file references a relative path, resolve it against the skill directory (parent of SKILL.md / dirname of the path) and use that absolute path in tool commands.".into(),
        String::new(),
        "<available_skills>".into(),
    ];

    for skill in &visible {
        lines.push("  <skill>".into());
        lines.push(format!("    <name>{}</name>", escape_xml(&skill.name)));
        lines.push(format!(
            "    <description>{}</description>",
            escape_xml(&skill.description)
        ));
        lines.push(format!("    <location>{}</location>", escape_xml(&skill.file_path)));
        lines.push("  </skill>".into());
    }

    lines.push("</available_skills>".into());
    lines.join("\n")
}

/// Return the default user skill data directory.
///
/// | Platform | Path |
/// |---|---|
/// | macOS / Linux | `~/.skill` |
/// | Windows | `%LOCALAPPDATA%\NeuroSkill` |
pub fn default_skill_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        dirs::data_local_dir()
            .unwrap_or_else(|| {
                std::env::var("APPDATA")
                    .map(PathBuf::from)
                    .unwrap_or_else(|_| std::env::temp_dir())
            })
            .join("NeuroSkill")
    }
    #[cfg(not(target_os = "windows"))]
    {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(skill_constants::SKILL_DIR)
    }
}

// ── Internal: directory scanning ──────────────────────────────────────────────

fn load_skills_from_dir(dir: &Path, source: &str, include_root_files: bool) -> LoadSkillsResult {
    load_skills_from_dir_inner(dir, source, include_root_files, dir)
}

fn load_skills_from_dir_inner(dir: &Path, source: &str, include_root_files: bool, root_dir: &Path) -> LoadSkillsResult {
    let mut skills = Vec::new();
    let mut diagnostics = Vec::new();

    if !dir.exists() || !dir.is_dir() {
        return LoadSkillsResult { skills, diagnostics };
    }

    // Build ignore matcher from .gitignore / .ignore / .fdignore in this dir.
    let ig = build_ignore(dir, root_dir);

    let Ok(entries) = fs::read_dir(dir) else {
        return LoadSkillsResult { skills, diagnostics };
    };

    let mut entries_vec: Vec<fs::DirEntry> = entries.filter_map(std::result::Result::ok).collect();
    entries_vec.sort_by_key(std::fs::DirEntry::file_name);

    // Phase 1: check for SKILL.md in this directory.
    let mut _is_index = false;
    for entry in &entries_vec {
        let name = entry.file_name();
        if name.to_str() != Some(SKILL_MARKER) {
            continue;
        }
        let full_path = entry.path();
        if !is_file_follow_symlinks(&full_path) {
            continue;
        }
        if is_ignored(&ig, &full_path, root_dir, false) {
            continue;
        }

        // Peek at the frontmatter to check for index: true before loading.
        let index_flag = fs::read_to_string(&full_path)
            .ok()
            .map(|c| {
                let (fm, _) = parse_frontmatter(&c);
                fm.get("index").and_then(serde_json::Value::as_bool).unwrap_or(false)
            })
            .unwrap_or(false);

        let (skill, diags) = load_skill_from_file(&full_path, source);
        diagnostics.extend(diags);
        if let Some(s) = skill {
            skills.push(s);
            if index_flag {
                // Index SKILL.md — load the skill but continue recursing
                // into subdirectories to find child skills.
                _is_index = true;
                break;
            }
            // Valid SKILL.md found — this dir is a skill root; do not recurse.
            return LoadSkillsResult { skills, diagnostics };
        }
        // SKILL.md exists but failed validation (e.g. missing description).
        // Continue to recurse into subdirectories — this supports index-style
        // SKILL.md files (e.g. a git submodule root) that contain child skills.
        _is_index = true;
        break;
    }

    // Phase 2: no valid SKILL.md (or index) — scan children.
    for entry in &entries_vec {
        let name_os = entry.file_name();
        let Some(name) = name_os.to_str() else {
            continue;
        };

        // Skip SKILL.md — already handled in Phase 1.
        if name == SKILL_MARKER {
            continue;
        }

        if name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }

        let full_path = entry.path();

        if is_dir_follow_symlinks(&full_path) {
            if is_ignored(&ig, &full_path, root_dir, true) {
                continue;
            }
            let sub = load_skills_from_dir_inner(&full_path, source, false, root_dir);
            skills.extend(sub.skills);
            diagnostics.extend(sub.diagnostics);
            continue;
        }

        if !include_root_files {
            continue;
        }

        if !is_file_follow_symlinks(&full_path) {
            continue;
        }

        if !name.ends_with(".md") {
            continue;
        }

        if is_ignored(&ig, &full_path, root_dir, false) {
            continue;
        }

        let (skill, diags) = load_skill_from_file(&full_path, source);
        diagnostics.extend(diags);
        if let Some(s) = skill {
            skills.push(s);
        }
    }

    LoadSkillsResult { skills, diagnostics }
}

// ── Internal: single file loading ─────────────────────────────────────────────

fn load_skill_from_file(path: &Path, source: &str) -> (Option<Skill>, Vec<SkillDiagnostic>) {
    let mut diags = Vec::new();
    let path_str = path.display().to_string();

    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            diags.push(SkillDiagnostic {
                level: "warning".into(),
                message: format!("failed to read: {e}"),
                path: path_str,
            });
            return (None, diags);
        }
    };

    let (frontmatter, _body) = parse_frontmatter(&content);
    let skill_dir = path.parent().unwrap_or(Path::new("."));
    let parent_dir_name = skill_dir.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // Description is required.
    let description = frontmatter
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if description.trim().is_empty() {
        diags.push(SkillDiagnostic {
            level: "warning".into(),
            message: "description is required in frontmatter".into(),
            path: path_str.clone(),
        });
        return (None, diags);
    }

    if description.len() > MAX_DESCRIPTION_LENGTH {
        diags.push(SkillDiagnostic {
            level: "warning".into(),
            message: format!(
                "description exceeds {MAX_DESCRIPTION_LENGTH} characters ({})",
                description.len()
            ),
            path: path_str.clone(),
        });
    }

    // Name: from frontmatter or parent directory.
    let name = frontmatter
        .get("name")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(std::string::ToString::to_string)
        .unwrap_or_else(|| parent_dir_name.to_string());

    // Validate name.
    if name.len() > MAX_NAME_LENGTH {
        diags.push(SkillDiagnostic {
            level: "warning".into(),
            message: format!("name exceeds {MAX_NAME_LENGTH} characters ({})", name.len()),
            path: path_str.clone(),
        });
    }

    let disable = frontmatter
        .get("disable-model-invocation")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    (
        Some(Skill {
            name,
            description,
            file_path: path_str.clone(),
            base_dir: skill_dir.display().to_string(),
            source: source.to_string(),
            disable_model_invocation: disable,
        }),
        diags,
    )
}

// ── Frontmatter parser ────────────────────────────────────────────────────────

/// Minimal YAML frontmatter parser.
///
/// Extracts key-value pairs from `---` delimited frontmatter.
/// Only supports top-level string and boolean values (sufficient for skill metadata).
fn parse_frontmatter(content: &str) -> (serde_json::Map<String, serde_json::Value>, &str) {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return (serde_json::Map::new(), content);
    }

    let after_open = &trimmed[3..];
    let end = after_open.find("\n---");
    let Some(end_pos) = end else {
        return (serde_json::Map::new(), content);
    };

    let fm_block = &after_open[..end_pos];
    let body_start = 3 + end_pos + 4; // "---" + block + "\n---"
    let body = if body_start < trimmed.len() {
        &trimmed[body_start..]
    } else {
        ""
    };

    let mut map = serde_json::Map::new();
    for line in fm_block.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, val)) = line.split_once(':') {
            let key = key.trim().to_string();
            let val = val.trim();
            // Strip quotes.
            let val = val
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .or_else(|| val.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
                .unwrap_or(val);
            match val {
                "true" => {
                    map.insert(key, serde_json::Value::Bool(true));
                }
                "false" => {
                    map.insert(key, serde_json::Value::Bool(false));
                }
                _ => {
                    map.insert(key, serde_json::Value::String(val.to_string()));
                }
            }
        }
    }

    (map, body)
}

// ── Ignore / gitignore support ────────────────────────────────────────────────

fn build_ignore(dir: &Path, _root_dir: &Path) -> Option<ignore::gitignore::Gitignore> {
    let mut builder = GitignoreBuilder::new(dir);
    let mut found = false;
    for name in &[".gitignore", ".ignore", ".fdignore"] {
        let p = dir.join(name);
        if p.exists() && builder.add(&p).is_none() {
            found = true;
        }
    }
    if found {
        builder.build().ok()
    } else {
        None
    }
}

fn is_ignored(ig: &Option<ignore::gitignore::Gitignore>, path: &Path, _root: &Path, is_dir: bool) -> bool {
    let Some(ref gi) = ig else { return false };
    gi.matched(path, is_dir).is_ignore()
}

// ── Path helpers ──────────────────────────────────────────────────────────────

fn is_file_follow_symlinks(path: &Path) -> bool {
    fs::metadata(path).map(|m| m.is_file()).unwrap_or(false)
}

fn is_dir_follow_symlinks(path: &Path) -> bool {
    fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false)
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parse_frontmatter_basic() {
        let content = r#"---
name: my-skill
description: A helpful skill
---
# Body content here
"#;
        let (fm, body) = parse_frontmatter(content);
        assert_eq!(fm.get("name").unwrap().as_str().unwrap(), "my-skill");
        assert_eq!(fm.get("description").unwrap().as_str().unwrap(), "A helpful skill");
        assert!(body.contains("Body content"));
    }

    #[test]
    fn parse_frontmatter_boolean() {
        let content = "---\ndisable-model-invocation: true\ndescription: test\n---\nbody";
        let (fm, _) = parse_frontmatter(content);
        assert_eq!(fm.get("disable-model-invocation").unwrap().as_bool(), Some(true));
    }

    #[test]
    fn parse_frontmatter_missing() {
        let content = "# Just markdown\nNo frontmatter here.";
        let (fm, body) = parse_frontmatter(content);
        assert!(fm.is_empty());
        assert_eq!(body, content);
    }

    #[test]
    fn load_skill_from_file_missing_description() {
        let dir = std::env::temp_dir().join("skill_test_no_desc");
        let _ = fs::create_dir_all(&dir);
        let file = dir.join("SKILL.md");
        fs::write(&file, "---\nname: test\n---\n# No description").unwrap();

        let (skill, diags) = load_skill_from_file(&file, "test");
        assert!(skill.is_none());
        assert!(diags.iter().any(|d| d.message.contains("description")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_skill_from_file_valid() {
        let dir = std::env::temp_dir().join("skill_test_valid");
        let _ = fs::create_dir_all(&dir);
        let file = dir.join("SKILL.md");
        fs::write(
            &file,
            "---\nname: my-skill\ndescription: Does something useful\n---\n# Instructions",
        )
        .unwrap();

        let (skill, diags) = load_skill_from_file(&file, "test");
        assert!(skill.is_some());
        let s = skill.unwrap();
        assert_eq!(s.name, "my-skill");
        assert_eq!(s.description, "Does something useful");
        assert!(!s.disable_model_invocation);
        assert!(diags.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn discover_skill_in_subdirectory() {
        let root = std::env::temp_dir().join("skill_test_discover");
        let sub = root.join("my-skill");
        let _ = fs::create_dir_all(&sub);
        fs::write(
            sub.join("SKILL.md"),
            "---\nname: my-skill\ndescription: A test skill\n---\n# Content",
        )
        .unwrap();

        let result = load_skills_from_dir(&root, "test", true);
        assert_eq!(result.skills.len(), 1);
        assert_eq!(result.skills[0].name, "my-skill");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn format_skills_xml() {
        let skills = vec![Skill {
            name: "test-skill".into(),
            description: "A test skill".into(),
            file_path: "/tmp/test/SKILL.md".into(),
            base_dir: "/tmp/test".into(),
            source: "test".into(),
            disable_model_invocation: false,
        }];
        let output = format_skills_for_prompt(&skills);
        assert!(output.contains("<available_skills>"));
        assert!(output.contains("<name>test-skill</name>"));
        assert!(output.contains("<description>A test skill</description>"));
        assert!(output.contains("</available_skills>"));
    }

    #[test]
    fn invalid_skill_md_allows_recursion() {
        // When a directory has a SKILL.md without a description (e.g. an
        // index file in a git submodule root), the scanner should continue
        // recursing into subdirectories to find valid skills.
        let root = std::env::temp_dir().join("skill_test_index_recurse");
        let _ = fs::remove_dir_all(&root);
        let _ = fs::create_dir_all(&root);
        // Root has a SKILL.md without description (index-style)
        fs::write(root.join("SKILL.md"), "# Skill Index\nNo frontmatter here.").unwrap();
        // Sub-skill with valid SKILL.md
        let sub = root.join("skills").join("my-sub-skill");
        let _ = fs::create_dir_all(&sub);
        fs::write(
            sub.join("SKILL.md"),
            "---\nname: my-sub-skill\ndescription: A valid sub-skill\n---\n# Content",
        )
        .unwrap();

        let result = load_skills_from_dir(&root, "test", true);
        assert_eq!(
            result.skills.len(),
            1,
            "expected 1 skill, got: {:?}",
            result.skills.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        assert_eq!(result.skills[0].name, "my-sub-skill");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn discover_real_skills_submodule() {
        // Test against the actual skills/ git submodule if present.
        let skills_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap() // crates/
            .parent()
            .unwrap() // project root
            .join("skills");
        if !skills_dir.exists() || skills_dir.read_dir().map_or(true, |mut d| d.next().is_none()) {
            return; // skip if submodule not checked out or empty
        }
        let result = load_skills_from_dir(&skills_dir, "bundled", true);
        assert!(
            result.skills.len() >= 10,
            "expected at least 10 skills from the submodule, got {}: {:?}",
            result.skills.len(),
            result.skills.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        // Check that neuroskill-status is among them.
        assert!(
            result.skills.iter().any(|s| s.name == "neuroskill-status"),
            "expected neuroskill-status skill, found: {:?}",
            result.skills.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn index_skill_md_allows_recursion() {
        // When a directory has a SKILL.md with `index: true` and a valid
        // description, the scanner should load it as a skill AND continue
        // recursing into subdirectories to find child skills.
        let root = std::env::temp_dir().join("skill_test_index_flag");
        let _ = fs::remove_dir_all(&root);
        let _ = fs::create_dir_all(&root);
        // Root has a SKILL.md with description AND index: true
        fs::write(
            root.join("SKILL.md"),
            "---\nname: my-index\ndescription: Skill index overview\nindex: true\n---\n# Index",
        )
        .unwrap();
        // Sub-skill with valid SKILL.md
        let sub = root.join("child-skill");
        let _ = fs::create_dir_all(&sub);
        fs::write(
            sub.join("SKILL.md"),
            "---\nname: child-skill\ndescription: A child skill\n---\n# Content",
        )
        .unwrap();

        let result = load_skills_from_dir(&root, "test", true);
        assert_eq!(
            result.skills.len(),
            2,
            "expected 2 skills (index + child), got: {:?}",
            result.skills.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        assert!(result.skills.iter().any(|s| s.name == "my-index"));
        assert!(result.skills.iter().any(|s| s.name == "child-skill"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn format_skills_excludes_disabled() {
        let skills = vec![Skill {
            name: "hidden".into(),
            description: "Should not appear".into(),
            file_path: "/tmp/hidden/SKILL.md".into(),
            base_dir: "/tmp/hidden".into(),
            source: "test".into(),
            disable_model_invocation: true,
        }];
        let output = format_skills_for_prompt(&skills);
        assert!(output.is_empty());
    }
}
