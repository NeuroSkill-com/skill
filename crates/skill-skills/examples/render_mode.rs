// SPDX-License-Identifier: GPL-3.0-only
//! Print the exact skills block a given `SkillPromptMode` produces.
//!
//! Used by `scripts/eval-skill-presets.py` so the model is scored against the
//! real rendering rather than a re-implementation of it.
use skill_skills::{format_skills_for_prompt_mode, load_skills, LoadSkillsOptions, SkillPromptMode};

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "skills/skills".into());
    let mode = std::env::args().nth(2).unwrap_or_else(|| "compact".into());
    let skills = load_skills(&LoadSkillsOptions {
        skill_paths: vec![std::path::PathBuf::from(&dir)],
        include_defaults: false,
        ..Default::default()
    })
    .skills;
    print!(
        "{}",
        format_skills_for_prompt_mode(&skills, SkillPromptMode::from_str_or_default(&mode))
    );
}
