// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! `skill-skills` — Agent Skills discovery, parsing, and prompt injection.
//!
//! Discovers `SKILL.md` files from multiple locations and makes them available
//! to the LLM chat so it can load specialised instructions on demand.
//!
//! ## Discovery locations (priority order)
//!
//! 1. **Project-local**: `<cwd>/.skill/skills/`
//! 2. **User-global**: `~/.skill/skills/`
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
/// name; the first loaded wins (project > user > bundled > explicit paths).
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
        // 1. Project-local (highest priority): <cwd>/.skill/skills/ AND <cwd>/.skill/ root
        let project_base = options.cwd.join(skill_constants::SKILL_DIR);
        let project_skills_dir = project_base.join(SKILLS_SUBDIR);
        add(load_skills_from_dir(&project_skills_dir, "project", true));
        add(load_skills_from_dir(&project_base, "project", true));

        // 2. User-global: ~/.skill/skills/ AND ~/.skill/ root
        let user_skills_dir = options.skill_dir.join(SKILLS_SUBDIR);
        add(load_skills_from_dir(&user_skills_dir, "user", true));
        // Also scan skill_dir root itself so users can drop SKILL.md files
        // directly into ~/.skill/ (or a custom data dir) without the skills/ subdir.
        add(load_skills_from_dir(&options.skill_dir, "user", true));

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

    // Collapse collision diagnostics into a single summary line when there are
    // multiple collisions, to avoid flooding the log with one line per skill.
    if collision_diags.len() > 1 {
        let names: Vec<String> = collision_diags
            .iter()
            .map(|d| {
                // Extract the skill name from the message: skill name "foo" collision: ...
                d.message.split('"').nth(1).unwrap_or("?").to_string()
            })
            .collect();
        all_diags.push(SkillDiagnostic {
            level: "collision".into(),
            message: format!(
                "{} skill name collisions (winner keeps priority): {}",
                names.len(),
                names.join(", "),
            ),
            path: String::new(),
        });
    } else {
        all_diags.extend(collision_diags);
    }
    // Sorted by name, because `HashMap::into_values` has no defined order and
    // Rust seeds its hasher per process — so without this the skills come out
    // in a *different order on every daemon start*. Three consequences, all
    // real: the system prompt differs between restarts (which would defeat any
    // prompt-prefix cache), ties in the relevance ranking resolve arbitrarily
    // so a different set of skills survives the top-N cut, and no measurement
    // or bug report over this list is reproducible. Found by a retrieval eval
    // whose supposedly deterministic lexical baseline scored 12, 14 and 12 out
    // of 30 on three consecutive runs.
    let mut skills: Vec<Skill> = skill_map.into_values().collect();
    skills.sort_by(|a, b| a.name.cmp(&b.name));

    LoadSkillsResult {
        skills,
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
/// Rank skills against a prompt by embedding similarity and keep the top `top_n`.
///
/// Lexical overlap ([`rerank_skills_for_prompt`]) only fires when the user
/// happens to reuse a word from the description: "help me wind down before
/// bed" shares no token with a skill described as "sleep hygiene protocol",
/// so the cut becomes arbitrary exactly when it matters. Embeddings match on
/// meaning instead.
///
/// `prompt_vec` and `skill_vecs` must come from the **same** model, and
/// `skill_vecs[i]` must correspond to `skills[i]`; a length mismatch falls
/// back to the lexical ranking rather than silently mis-ranking.
///
/// This takes vectors rather than an embedder because the embedder lives in
/// the daemon and this crate is a leaf — see the ranker hook on
/// `LlmServerState`.
pub fn rank_skills_by_embedding(
    prompt: &str,
    skills: Vec<Skill>,
    prompt_vec: &[f32],
    skill_vecs: &[Vec<f32>],
    top_n: usize,
) -> Vec<Skill> {
    if skills.is_empty() || top_n == 0 || skills.len() <= top_n {
        return skills;
    }
    // A mismatch means the caches disagree about which skills are loaded;
    // ranking on that would pair a prompt with the wrong description.
    if skill_vecs.len() != skills.len() || prompt_vec.is_empty() {
        return rerank_skills_for_prompt(prompt, skills, top_n);
    }

    let mut scored: Vec<(usize, f32, Skill)> = skills
        .into_iter()
        .enumerate()
        .map(|(i, s)| (i, cosine(prompt_vec, &skill_vecs[i]), s))
        .collect();

    // Descending similarity, then catalog order so ties stay reproducible.
    scored.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    scored.into_iter().take(top_n).map(|(_, _, s)| s).collect()
}

/// Cosine similarity, 0.0 when either side has no magnitude.
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let (mut dot, mut na, mut nb) = (0f32, 0f32, 0f32);
    for (x, y) in a.iter().zip(b) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na <= 0.0 || nb <= 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// The text a skill is embedded as.
///
/// Name and description together: the name alone is too short to place
/// semantically, and the description alone loses a name that is often the
/// most on-topic phrase there is.
pub fn skill_embedding_text(skill: &Skill) -> String {
    format!("{}: {}", skill.name, skill.description)
}

/// Rerank skills for a prompt by lightweight lexical relevance and keep the
/// top `top_n`.
///
/// The direct analogue of `skill_tools::rerank_tools_for_prompt`, and it exists
/// for a sharper reason: the tool block is bounded by how many tools there are,
/// but the skills block grows with whatever the user has installed. Measured
/// through the daemon, 33 discovered skills came to ~3,900 prompt tokens — more
/// than twice the entire tool block — which is paid on *every* turn as prefill
/// latency, as a prefill graph sized for that prompt, and as small models
/// answering the system prompt instead of the user.
///
/// Scoring is deliberately cheap and dependency-free: overlap between prompt
/// tokens and each skill's name and description. Ties keep catalog order, so
/// the result is stable for the same input.
pub fn rerank_skills_for_prompt(prompt: &str, skills: Vec<Skill>, top_n: usize) -> Vec<Skill> {
    if skills.is_empty() || top_n == 0 || skills.len() <= top_n {
        return skills;
    }

    fn tokens(s: &str) -> std::collections::HashSet<String> {
        s.split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
            .filter(|t| !t.is_empty())
            .map(str::to_ascii_lowercase)
            .filter(|t| t.len() > 1)
            .collect()
    }

    let prompt_tokens = tokens(prompt);
    // Nothing to match on — keep the first `top_n` rather than scoring noise.
    if prompt_tokens.is_empty() {
        return skills.into_iter().take(top_n).collect();
    }

    let mut scored: Vec<(usize, i32, Skill)> = skills
        .into_iter()
        .enumerate()
        .map(|(idx, skill)| {
            let name_tokens = tokens(&skill.name);
            let desc_tokens = tokens(&skill.description);
            let mut score = 0i32;
            for t in &prompt_tokens {
                // A name hit is worth more than a description hit: a skill
                // named for the task is a stronger signal than one that merely
                // mentions the word somewhere in its blurb.
                if name_tokens.contains(t) {
                    score += 3;
                }
                if desc_tokens.contains(t) {
                    score += 1;
                }
            }
            (idx, score, skill)
        })
        .collect();

    // Descending score, then original order — a stable, reproducible cut.
    scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    scored.into_iter().take(top_n).map(|(_, _, s)| s).collect()
}

/// How the available-skills block is rendered into the system prompt.
///
/// This block is paid on **every turn**, before the user has typed anything,
/// and it grows with whatever is installed: 30 bundled skills measure ~3,519
/// tokens in [`SkillPromptMode::Full`]. That is charged three times over — as
/// prefill latency before the first byte, as a prefill graph sized for the
/// larger prompt, and as a small model answering the system prompt instead of
/// the question.
///
/// The modes trade prompt size against how much the model knows up front.
/// Measured on the 30 bundled skills:
///
/// | mode | tokens | of `Full` | what it gives up |
/// |---|---|---|---|
/// | [`Full`](Self::Full) | 3519 | 100% | nothing |
/// | [`Compact`](Self::Compact) | 2333 | 66% | nothing — pure formatting |
/// | [`Grouped`](Self::Grouped) | ~930 | 26% | a round trip to open a family |
/// | [`Brief`](Self::Brief) | ~1384 | 39% | description detail |
/// | [`NamesOnly`](Self::NamesOnly) | 176 | 5% | a round trip for every skill |
///
/// `Full` is kept because it is the only mode that is certainly lossless for
/// a model that parses XML better than prose; `Compact` is the default
/// because the 34% it saves is scaffolding and absolute paths, which carry no
/// information the model cannot reconstruct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillPromptMode {
    /// One XML element per skill, with its absolute path. The original format.
    Full,
    /// `name: description`, one per line, with the base directory stated once.
    ///
    /// Lossless: the `<location>` of every skill is `<base>/<name>/SKILL.md`,
    /// so repeating a ~65-character absolute path per entry — 14% of the block
    /// — tells the model nothing it cannot derive.
    Compact,
    /// [`Compact`](Self::Compact), with families collapsed to one entry.
    ///
    /// 14 of the 30 bundled skills are `neuroskill-protocols-*`, one family
    /// competing with itself for the model's attention. It becomes a single
    /// line naming the family and its members.
    Grouped,
    /// [`Compact`](Self::Compact), with each description truncated.
    Brief,
    /// Names only; the model reads a skill's file to learn what it does.
    NamesOnly,
    /// Only the skills most relevant to this message, rendered compact.
    ///
    /// The only mode that *selects* rather than just reformatting: the list is
    /// cut to the most relevant few per message, so its size stops tracking
    /// how many skills are installed. Measured on the bundled set it is ~690
    /// tokens against 2,214 for [`Compact`](Self::Compact) — and because a
    /// long prompt is charged twice (prefill, then every decoded token
    /// attending over it), that is worth far more than the token count alone
    /// suggests.
    ///
    /// The risk it carries that the others do not: a skill the ranker misses
    /// is invisible to the model. Embedding-based ranking keeps the right
    /// skill in the top 8 for 93% of paraphrased queries against 27% for word
    /// overlap, which is why it is the ranker to use here.
    #[default]
    Smart,
    /// [`Smart`](Self::Smart), with families collapsed as in
    /// [`Grouped`](Self::Grouped).
    ///
    /// Only bites when a selected set still contains several members of one
    /// family; with a small cut that is uncommon, so it usually renders the
    /// same as `Smart`.
    SmartGrouped,
}

/// Characters of description kept in [`SkillPromptMode::Brief`].
///
/// 160 is about two lines — enough for the "what it is" clause that usually
/// opens these descriptions, before the trigger lists that make up their bulk.
const BRIEF_DESCRIPTION_CHARS: usize = 160;

impl SkillPromptMode {
    /// Parse a settings string, falling back to the default rather than
    /// failing: an unknown mode in a config file should degrade, not break
    /// chat entirely.
    pub fn from_str_or_default(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "full" => Self::Full,
            // Explicit, not relying on the fallback: when the default moved
            // to `Smart`, a saved "compact" setting silently became `Smart`.
            "compact" => Self::Compact,
            "grouped" => Self::Grouped,
            "brief" => Self::Brief,
            "names_only" | "names-only" | "namesonly" => Self::NamesOnly,
            "smart" => Self::Smart,
            "smart_grouped" | "smart-grouped" => Self::SmartGrouped,
            _ => Self::Smart,
        }
    }

    /// The settings string for this mode.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Compact => "compact",
            Self::Grouped => "grouped",
            Self::Brief => "brief",
            Self::NamesOnly => "names_only",
            Self::Smart => "smart",
            Self::SmartGrouped => "smart_grouped",
        }
    }

    /// Whether this mode cuts the list to the most relevant skills.
    ///
    /// Only the `Smart*` modes do. The rest reformat the whole catalogue, so
    /// their size grows with however many skills are installed.
    pub fn selects(self) -> bool {
        matches!(self, Self::Smart | Self::SmartGrouped)
    }

    /// How a selecting mode lays out the skills it kept.
    fn layout(self) -> Self {
        match self {
            Self::Smart => Self::Compact,
            Self::SmartGrouped => Self::Grouped,
            other => other,
        }
    }
}

/// The directory every skill in `skills` sits under, when they share one.
///
/// Stated once in the compact modes so the per-entry path can be dropped.
fn common_base_dir(skills: &[&Skill]) -> Option<String> {
    let first = std::path::Path::new(&skills.first()?.base_dir).parent()?.to_path_buf();
    skills
        .iter()
        .all(|s| std::path::Path::new(&s.base_dir).parent() == Some(first.as_path()))
        .then(|| first.display().to_string())
}

/// The family a skill belongs to in [`SkillPromptMode::Grouped`] — everything
/// before the last `-`, so `neuroskill-protocols-focus` groups under
/// `neuroskill-protocols`.
fn family_of(name: &str) -> Option<&str> {
    let (head, _) = name.rsplit_once('-')?;
    head.contains('-').then_some(head)
}

/// Render the available-skills block in `mode`.
pub fn format_skills_for_prompt_mode(skills: &[Skill], mode: SkillPromptMode) -> String {
    // The `Smart*` modes differ from the others in *which* skills they are
    // handed, not in how those are laid out — the caller does the cut.
    let mode = mode.layout();
    if mode == SkillPromptMode::Full {
        return format_skills_for_prompt(skills);
    }

    let visible: Vec<&Skill> = skills.iter().filter(|s| !s.disable_model_invocation).collect();
    if visible.is_empty() {
        return String::new();
    }

    let base = common_base_dir(&visible);
    let mut out = String::from(
        "

The following skills provide specialised instructions for specific tasks.
",
    );
    out.push_str(
        "Use the read_file tool to load a skill's file when the task matches its description.
",
    );
    match &base {
        Some(dir) => out.push_str(&format!(
            "Each skill's file is {dir}/<name>/SKILL.md.
"
        )),
        None => out.push_str(
            "Each skill's file path is given after its name.
",
        ),
    }
    out.push_str(
        "When a skill file references a relative path, resolve it against that skill's directory.

<available_skills>
",
    );

    // Where no shared base dir exists, the path has to ride along per entry.
    let loc = |s: &Skill| -> String {
        if base.is_some() {
            String::new()
        } else {
            format!(" [{}]", s.file_path)
        }
    };

    match mode {
        // `layout()` has already mapped these away.
        SkillPromptMode::Full | SkillPromptMode::Smart | SkillPromptMode::SmartGrouped => {
            unreachable!("resolved by layout()")
        }
        SkillPromptMode::Compact => {
            for s in &visible {
                out.push_str(&format!(
                    "{}{}: {}
",
                    s.name,
                    loc(s),
                    s.description
                ));
            }
        }
        SkillPromptMode::Brief => {
            for s in &visible {
                let d = truncate_on_char_boundary(&s.description, BRIEF_DESCRIPTION_CHARS);
                out.push_str(&format!(
                    "{}{}: {}
",
                    s.name,
                    loc(s),
                    d
                ));
            }
        }
        SkillPromptMode::NamesOnly => {
            for s in &visible {
                out.push_str(&format!(
                    "{}{}
",
                    s.name,
                    loc(s)
                ));
            }
        }
        SkillPromptMode::Grouped => {
            let mut emitted_family: Vec<&str> = Vec::new();
            for s in &visible {
                match family_of(&s.name) {
                    // Only collapse a family that actually has more than one
                    // member — a lone `a-b-c` is not a family, and replacing
                    // it with a pointer would cost a round trip for nothing.
                    Some(fam)
                        if visible
                            .iter()
                            .filter(|o| family_of(&o.name) == Some(fam))
                            .nth(1)
                            .is_some() =>
                    {
                        if emitted_family.contains(&fam) {
                            continue;
                        }
                        emitted_family.push(fam);
                        let members: Vec<&str> = visible
                            .iter()
                            .filter(|o| family_of(&o.name) == Some(fam))
                            .map(|o| o.name.trim_start_matches(fam).trim_start_matches('-'))
                            .collect();
                        out.push_str(&format!(
                            "{fam}-<topic>: {} variants — {}. Read the one that matches.
",
                            members.len(),
                            members.join(", ")
                        ));
                    }
                    _ => out.push_str(&format!(
                        "{}{}: {}
",
                        s.name,
                        loc(s),
                        s.description
                    )),
                }
            }
        }
    }

    out.push_str("</available_skills>");
    out
}

/// Truncate to at most `max` bytes without splitting a UTF-8 character.
///
/// A naive `&s[..max]` panics the moment a description contains a non-ASCII
/// character, which several of them do.
fn truncate_on_char_boundary(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

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
#[allow(clippy::unwrap_used)]
mod rerank_tests {
    use super::*;

    fn skill(name: &str, description: &str) -> Skill {
        Skill {
            name: name.into(),
            description: description.into(),
            file_path: format!("/skills/{name}/SKILL.md"),
            base_dir: format!("/skills/{name}"),
            source: "user".into(),
            disable_model_invocation: false,
        }
    }

    fn many(n: usize) -> Vec<Skill> {
        (0..n).map(|i| skill(&format!("skill-{i}"), "generic helper")).collect()
    }

    /// Under the cap nothing is dropped or reordered — the cut must not
    /// perturb a catalog that already fits.
    #[test]
    fn a_small_catalog_is_returned_untouched() {
        let skills = many(5);
        let out = rerank_skills_for_prompt("anything", skills.clone(), 8);
        assert_eq!(out.len(), 5);
        assert_eq!(
            out.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
            skills.iter().map(|s| s.name.clone()).collect::<Vec<_>>()
        );
    }

    /// The point of the whole exercise: a large catalog is cut to the cap.
    #[test]
    fn a_large_catalog_is_capped() {
        assert_eq!(rerank_skills_for_prompt("anything", many(33), 8).len(), 8);
    }

    /// A name match outranks a description match, and both outrank nothing.
    #[test]
    fn relevance_beats_catalog_order() {
        let mut skills = many(20);
        skills.push(skill("mentions-pdf", "this one merely mentions pdf in passing"));
        skills.push(skill("pdf", "fill in and sign forms"));

        let out = rerank_skills_for_prompt("help me edit a pdf", skills, 2);
        let names: Vec<&str> = out.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names[0], "pdf", "a name hit must win");
        assert_eq!(names[1], "mentions-pdf", "a description hit comes next");
    }

    /// A prompt with nothing to match on must still be bounded, not bail out
    /// and hand back the whole catalog.
    #[test]
    fn an_unmatched_prompt_is_still_capped() {
        assert_eq!(rerank_skills_for_prompt("!!! ???", many(33), 8).len(), 8);
        assert_eq!(rerank_skills_for_prompt("", many(33), 8).len(), 8);
    }

    /// `top_n = 0` means "no cut configured" and must not silently erase every
    /// skill — that would look like skills had stopped being discovered.
    #[test]
    fn a_zero_cap_is_not_a_wipe() {
        assert_eq!(rerank_skills_for_prompt("anything", many(33), 0).len(), 33);
    }

    /// Equal scores keep catalog order, so the same input always yields the
    /// same prompt — a prompt that reshuffles defeats prefix caching.
    #[test]
    fn ties_are_stable() {
        let first = rerank_skills_for_prompt("generic", many(33), 8);
        let second = rerank_skills_for_prompt("generic", many(33), 8);
        assert_eq!(
            first.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
            second.iter().map(|s| s.name.clone()).collect::<Vec<_>>()
        );
    }

    /// What this is for: the measured cost of the block, before and after.
    #[test]
    fn capping_cuts_the_prompt_block_substantially() {
        let skills = (0..33)
            .map(|i| {
                skill(
                    &format!("skill-{i}"),
                    "A reasonably wordy description of what this skill does, of the \
                     kind a real catalog entry carries so the model can choose it.",
                )
            })
            .collect::<Vec<_>>();

        let full = format_skills_for_prompt(&skills);
        let capped = format_skills_for_prompt(&rerank_skills_for_prompt("hi", skills, 8));

        assert!(
            capped.len() * 2 < full.len(),
            "capping 33 skills to 8 should more than halve the block \
             (full {} chars, capped {} chars)",
            full.len(),
            capped.len()
        );
    }
}

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

    /// `HashMap` iteration order is unspecified and Rust seeds its hasher per
    /// process, so an unsorted result changes on every start — varying the
    /// system prompt, the ranking tiebreak and any measurement over it.
    #[test]
    fn load_order_is_deterministic() {
        let dir = std::env::current_dir().unwrap();
        let load = || {
            load_skills(&LoadSkillsOptions {
                skill_paths: vec![dir.join("../../skills/skills")],
                include_defaults: false,
                ..Default::default()
            })
            .skills
            .iter()
            .map(|s| s.name.clone())
            .collect::<Vec<_>>()
        };
        let first = load();
        if first.is_empty() {
            return; // bundled skills not present in this checkout
        }
        assert_eq!(first, load(), "two loads disagreed on order");

        let mut sorted = first.clone();
        sorted.sort();
        assert_eq!(first, sorted, "result must be sorted by name");
    }

    // ── Prompt modes ─────────────────────────────────────────────────────────

    fn skill(name: &str, description: &str) -> Skill {
        Skill {
            name: name.into(),
            description: description.into(),
            file_path: format!("/skills/{name}/SKILL.md"),
            base_dir: format!("/skills/{name}"),
            source: "user".into(),
            disable_model_invocation: false,
        }
    }

    fn many(n: usize) -> Vec<Skill> {
        (0..n).map(|i| skill(&format!("skill-{i}"), "generic helper")).collect()
    }

    fn family_set() -> Vec<Skill> {
        let mut v: Vec<Skill> = (0..4)
            .map(|i| {
                let mut s = skill(
                    &format!("ns-protocols-{i}"),
                    "a protocol with a reasonably long description",
                );
                s.base_dir = format!("/skills/ns-protocols-{i}");
                s.file_path = format!("/skills/ns-protocols-{i}/SKILL.md");
                s
            })
            .collect();
        for n in ["ns-status", "ns-labels"] {
            let mut s = skill(n, "a standalone skill with its own description");
            s.base_dir = format!("/skills/{n}");
            s.file_path = format!("/skills/{n}/SKILL.md");
            v.push(s);
        }
        v
    }

    /// Compact must keep every skill and every description — the only thing it
    /// drops is scaffolding and a path the model can reconstruct.
    #[test]
    fn compact_is_lossless_and_smaller() {
        let skills = family_set();
        let full = format_skills_for_prompt_mode(&skills, SkillPromptMode::Full);
        let compact = format_skills_for_prompt_mode(&skills, SkillPromptMode::Compact);

        for s in &skills {
            assert!(compact.contains(&s.name), "{} missing", s.name);
            assert!(compact.contains(&s.description), "{} description missing", s.name);
        }
        assert!(
            compact.len() < full.len(),
            "compact {} vs full {}",
            compact.len(),
            full.len()
        );
        // The shared base dir is stated once instead of per entry.
        assert_eq!(compact.matches("/skills/ns-status/SKILL.md").count(), 0);
        assert!(compact.contains("/skills/<name>/SKILL.md"));
    }

    /// Grouped collapses a real family but must leave a lone skill alone —
    /// replacing a single skill with a pointer costs a round trip for nothing.
    #[test]
    fn grouped_collapses_families_but_not_singletons() {
        let out = format_skills_for_prompt_mode(&family_set(), SkillPromptMode::Grouped);
        assert!(out.contains("ns-protocols-<topic>"), "family not collapsed: {out}");
        assert_eq!(out.matches("ns-protocols-").count(), 1, "family emitted more than once");
        // Standalone skills keep their own line and description.
        assert!(out.contains("ns-status: a standalone skill"));
        assert!(out.contains("ns-labels: a standalone skill"));
        assert!(out.len() < format_skills_for_prompt_mode(&family_set(), SkillPromptMode::Compact).len());
    }

    #[test]
    fn brief_truncates_descriptions_but_keeps_every_skill() {
        let mut skills = family_set();
        skills[0].description = "x".repeat(400);
        let out = format_skills_for_prompt_mode(&skills, SkillPromptMode::Brief);
        for s in &skills {
            assert!(out.contains(&s.name));
        }
        assert!(!out.contains(&"x".repeat(400)), "description was not truncated");
        assert!(out.contains(&"x".repeat(BRIEF_DESCRIPTION_CHARS)));
    }

    /// Truncation must not split a multi-byte character — several descriptions
    /// contain them, and `&s[..n]` would panic.
    #[test]
    fn brief_truncation_is_utf8_safe() {
        let mut skills = family_set();
        skills[0].description = "é".repeat(300);
        let out = format_skills_for_prompt_mode(&skills, SkillPromptMode::Brief);
        assert!(out.contains("é"));
    }

    #[test]
    fn names_only_drops_descriptions() {
        let skills = family_set();
        let out = format_skills_for_prompt_mode(&skills, SkillPromptMode::NamesOnly);
        for s in &skills {
            assert!(out.contains(&s.name));
        }
        assert!(!out.contains("a standalone skill with its own description"));
    }

    /// The ordering the presets are offered in should match what they cost.
    ///
    /// Descriptions here are ~290 characters, the median of the bundled
    /// skills — with short ones `Brief` is correctly a no-op, which would make
    /// this assertion test the fixture rather than the modes.
    #[test]
    fn the_modes_are_ordered_by_size() {
        let mut s = family_set();
        for sk in &mut s {
            sk.description = "A realistically wordy description of what this skill covers and the \
                              situations that should bring it to mind, which is what makes these \
                              entries expensive in the first place."
                .into();
        }
        let len = |m| format_skills_for_prompt_mode(&s, m).len();
        assert!(len(SkillPromptMode::Full) > len(SkillPromptMode::Compact));
        assert!(len(SkillPromptMode::Compact) > len(SkillPromptMode::Brief));
        assert!(len(SkillPromptMode::Brief) > len(SkillPromptMode::NamesOnly));
        assert!(len(SkillPromptMode::Compact) > len(SkillPromptMode::Grouped));
    }

    /// `Brief` only shrinks what is over the cap — a short description is
    /// already brief, and truncating it would gain nothing.
    #[test]
    fn brief_is_a_no_op_for_short_descriptions() {
        let s = family_set(); // descriptions well under the cap
        assert_eq!(
            format_skills_for_prompt_mode(&s, SkillPromptMode::Brief),
            format_skills_for_prompt_mode(&s, SkillPromptMode::Compact)
        );
    }

    /// A hidden skill stays hidden in every mode.
    #[test]
    fn disabled_skills_never_appear() {
        let mut skills = family_set();
        skills[0].disable_model_invocation = true;
        let hidden = skills[0].name.clone();
        for m in [
            SkillPromptMode::Full,
            SkillPromptMode::Compact,
            SkillPromptMode::Grouped,
            SkillPromptMode::Brief,
            SkillPromptMode::NamesOnly,
            SkillPromptMode::Smart,
            SkillPromptMode::SmartGrouped,
        ] {
            assert!(
                !format_skills_for_prompt_mode(&skills, m).contains(&hidden),
                "{m:?} leaked it"
            );
        }
    }

    // ── Selecting presets ────────────────────────────────────────────────────

    /// Only the `Smart*` modes cut the list. Getting this wrong either hides
    /// skills under a mode that promised to show them all, or leaves `Smart`
    /// showing everything and costing what it was chosen to avoid.
    #[test]
    fn only_the_smart_modes_select() {
        for m in [
            SkillPromptMode::Full,
            SkillPromptMode::Compact,
            SkillPromptMode::Grouped,
            SkillPromptMode::Brief,
            SkillPromptMode::NamesOnly,
        ] {
            assert!(!m.selects(), "{m:?} must render every skill");
        }
        assert!(SkillPromptMode::Smart.selects());
        assert!(SkillPromptMode::SmartGrouped.selects());
    }

    /// `Smart` lays out whatever it was handed exactly as `Compact` does, and
    /// `SmartGrouped` as `Grouped` — the difference is the caller's cut, not
    /// the rendering.
    #[test]
    fn smart_modes_reuse_the_existing_layouts() {
        let s = family_set();
        assert_eq!(
            format_skills_for_prompt_mode(&s, SkillPromptMode::Smart),
            format_skills_for_prompt_mode(&s, SkillPromptMode::Compact)
        );
        assert_eq!(
            format_skills_for_prompt_mode(&s, SkillPromptMode::SmartGrouped),
            format_skills_for_prompt_mode(&s, SkillPromptMode::Grouped)
        );
    }

    /// The default must be the one mode whose size does not grow with however
    /// many skills the user installs.
    #[test]
    fn the_default_is_a_selecting_mode() {
        assert_eq!(SkillPromptMode::default(), SkillPromptMode::Smart);
        assert!(SkillPromptMode::default().selects());
    }

    #[test]
    fn mode_strings_round_trip_and_unknown_falls_back() {
        for m in [
            SkillPromptMode::Full,
            SkillPromptMode::Compact,
            SkillPromptMode::Grouped,
            SkillPromptMode::Brief,
            SkillPromptMode::NamesOnly,
            SkillPromptMode::Smart,
            SkillPromptMode::SmartGrouped,
        ] {
            assert_eq!(SkillPromptMode::from_str_or_default(m.as_str()), m);
        }
        // A stale or mistyped config degrades rather than breaking chat.
        assert_eq!(SkillPromptMode::from_str_or_default("nonsense"), SkillPromptMode::Smart);
        assert_eq!(SkillPromptMode::default(), SkillPromptMode::Smart);
    }

    // ── Semantic ranking ─────────────────────────────────────────────────────

    /// The case word overlap cannot serve: no shared token, right answer.
    #[test]
    fn embeddings_rank_by_meaning_not_shared_words() {
        let skills = vec![
            skill("ns-labels", "label management"),
            skill("ns-sleep", "sleep hygiene protocol"),
            skill("ns-export", "export data to csv"),
        ];
        // Stand-in vectors: "wind down before bed" sits nearest sleep.
        let prompt_vec = vec![0.0, 1.0, 0.0];
        let skill_vecs = vec![vec![1.0, 0.0, 0.0], vec![0.1, 0.99, 0.0], vec![0.0, 0.0, 1.0]];

        let out = rank_skills_by_embedding(
            "help me wind down before bed",
            skills.clone(),
            &prompt_vec,
            &skill_vecs,
            1,
        );
        assert_eq!(out[0].name, "ns-sleep");

        // Word overlap, given the same prompt, cannot find it.
        let lex = rerank_skills_for_prompt("help me wind down before bed", skills, 1);
        assert_ne!(lex[0].name, "ns-sleep", "lexical should miss — that is the motivation");
    }

    /// A vector/skill length mismatch means the caches disagree; ranking on it
    /// would pair a prompt with the wrong description.
    #[test]
    fn a_vector_mismatch_falls_back_instead_of_mis_ranking() {
        let skills = many(20);
        let out = rank_skills_by_embedding("anything", skills.clone(), &[1.0, 0.0], &[vec![1.0, 0.0]], 5);
        assert_eq!(out.len(), 5, "must still cap via the lexical fallback");
    }

    #[test]
    fn an_empty_prompt_vector_falls_back() {
        let out = rank_skills_by_embedding("anything", many(20), &[], &[], 5);
        assert_eq!(out.len(), 5);
    }

    #[test]
    fn embedding_text_carries_name_and_description() {
        let s = skill("ns-sleep", "sleep hygiene protocol");
        assert_eq!(skill_embedding_text(&s), "ns-sleep: sleep hygiene protocol");
    }
}
