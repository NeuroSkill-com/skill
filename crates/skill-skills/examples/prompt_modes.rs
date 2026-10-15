// SPDX-License-Identifier: GPL-3.0-only
//! Print what each `SkillPromptMode` costs against the real skills on disk.
//!
//! ```sh
//! cargo run -p skill-skills --example prompt_modes -- skills/skills
//! ```
use skill_skills::{format_skills_for_prompt_mode, load_skills, LoadSkillsOptions, SkillPromptMode};

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "skills/skills".into());
    let loaded = load_skills(&LoadSkillsOptions {
        skill_paths: vec![std::path::PathBuf::from(&dir)],
        include_defaults: false,
        ..Default::default()
    });
    let skills = loaded.skills;
    println!("{} skills from {dir}\n", skills.len());

    let full = format_skills_for_prompt_mode(&skills, SkillPromptMode::Full).len();
    for mode in [
        SkillPromptMode::Full,
        SkillPromptMode::Compact,
        SkillPromptMode::Grouped,
        SkillPromptMode::Brief,
        SkillPromptMode::NamesOnly,
    ] {
        let n = format_skills_for_prompt_mode(&skills, mode).len();
        println!(
            "  {:<10} {:>6} chars  ≈{:>5} tokens   {:>5.1}% of full",
            mode.as_str(),
            n,
            n / 4,
            100.0 * n as f64 / full.max(1) as f64
        );
    }
}
