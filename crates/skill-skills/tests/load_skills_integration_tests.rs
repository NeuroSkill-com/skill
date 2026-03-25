// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

use std::fs;
use std::path::PathBuf;

use skill_skills::{format_skills_for_prompt, load_skills, LoadSkillsOptions};

fn temp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    p.push(format!("skill-skills-it-{tag}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&p).expect("create temp dir");
    p
}

#[test]
fn load_skills_from_explicit_path_includes_valid_skill() {
    let root = temp_dir("valid");
    let skill_dir = root.join("my-skill");
    fs::create_dir_all(&skill_dir).expect("create skill dir");
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: my-skill\ndescription: Integration test skill\n---\n# Use me\n",
    )
    .expect("write skill");

    let options = LoadSkillsOptions {
        cwd: root.clone(),
        skill_dir: root.join(".skill-home"),
        bundled_dir: None,
        skill_paths: vec![skill_dir.clone()],
        include_defaults: false,
    };

    let result = load_skills(&options);
    assert_eq!(result.skills.len(), 1);
    assert_eq!(result.skills[0].name, "my-skill");
    assert_eq!(result.skills[0].description, "Integration test skill");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn load_skills_skips_missing_description_with_warning() {
    let root = temp_dir("missing-desc");
    let skill_dir = root.join("bad-skill");
    fs::create_dir_all(&skill_dir).expect("create skill dir");
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: bad-skill\n---\n# Missing description\n",
    )
    .expect("write skill");

    let options = LoadSkillsOptions {
        cwd: root.clone(),
        skill_dir: root.join(".skill-home"),
        bundled_dir: None,
        skill_paths: vec![skill_dir.clone()],
        include_defaults: false,
    };

    let result = load_skills(&options);
    assert!(result.skills.is_empty());
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.message.contains("description is required")));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn format_skills_for_prompt_omits_disabled_skills() {
    let root = temp_dir("prompt");
    let a = root.join("a");
    let b = root.join("b");
    fs::create_dir_all(&a).expect("create a");
    fs::create_dir_all(&b).expect("create b");

    fs::write(
        a.join("SKILL.md"),
        "---\nname: visible-skill\ndescription: Visible skill\n---\n# A\n",
    )
    .expect("write a");
    fs::write(
        b.join("SKILL.md"),
        "---\nname: hidden-skill\ndescription: Hidden skill\ndisable-model-invocation: true\n---\n# B\n",
    )
    .expect("write b");

    let options = LoadSkillsOptions {
        cwd: root.clone(),
        skill_dir: root.join(".skill-home"),
        bundled_dir: None,
        skill_paths: vec![a, b],
        include_defaults: false,
    };

    let result = load_skills(&options);
    let prompt = format_skills_for_prompt(&result.skills);

    assert!(prompt.contains("visible-skill"));
    assert!(!prompt.contains("hidden-skill"));

    let _ = fs::remove_dir_all(root);
}
