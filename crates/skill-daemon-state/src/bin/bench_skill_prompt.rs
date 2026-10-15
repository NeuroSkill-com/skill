// SPDX-License-Identifier: GPL-3.0-only
//! Where does the per-turn skills-prompt work actually go?
//!
//! Precomputing the wrong thing is just complexity. This times each stage of
//! assembling the skills block so the candidates can be ranked by what they
//! would actually save.
//!
//! ```sh
//! cargo run --release -p skill-daemon-state --features text-embeddings-rlx-metal \
//!   --bin bench_skill_prompt -- skills/skills
//! ```
use skill_daemon_state::text_embedder::SharedTextEmbedder;
use skill_skills::{
    format_skills_for_prompt_mode, load_skills, rank_skills_by_embedding, skill_embedding_text, LoadSkillsOptions,
    SkillPromptMode,
};
use std::time::Instant;

const QUERY: &str = "help me wind down before bed";
const RUNS: u32 = 20;

fn ms(f: impl Fn()) -> f64 {
    // One warm pass so the first allocation is not charged to the mean.
    f();
    let t = Instant::now();
    for _ in 0..RUNS {
        f();
    }
    t.elapsed().as_secs_f64() * 1000.0 / f64::from(RUNS)
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "skills/skills".into());

    let t = Instant::now();
    let skills = load_skills(&LoadSkillsOptions {
        skill_paths: vec![std::path::PathBuf::from(&dir)],
        include_defaults: false,
        ..Default::default()
    })
    .skills;
    let load_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!("{} skills\n", skills.len());

    let embedder = SharedTextEmbedder::new();
    let texts: Vec<String> = skills.iter().map(skill_embedding_text).collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();

    // Cold: loading the embedding model, then embedding every skill.
    let t = Instant::now();
    let vecs = embedder.embed_documents(refs.clone());
    let cold_ms = t.elapsed().as_secs_f64() * 1000.0;
    let Some(vecs) = vecs else {
        println!("embedder unavailable — rerun with --features text-embeddings-rlx-metal");
        return;
    };

    // Warm: re-embedding the same skills with the model already resident.
    let warm_batch_ms = {
        let t = Instant::now();
        let _ = embedder.embed_documents(refs);
        t.elapsed().as_secs_f64() * 1000.0
    };

    let query_ms = ms(|| {
        let _ = embedder.embed_query(QUERY);
    });
    let pv = embedder.embed_query(QUERY).expect("query vector");
    let rank_ms = ms(|| {
        let _ = rank_skills_by_embedding(QUERY, skills.clone(), &pv, &vecs, 8);
    });
    let picked = rank_skills_by_embedding(QUERY, skills.clone(), &pv, &vecs, 8);
    let render_ms = ms(|| {
        let _ = format_skills_for_prompt_mode(&picked, SkillPromptMode::Compact);
    });
    let render_all_ms = ms(|| {
        let _ = format_skills_for_prompt_mode(&skills, SkillPromptMode::Compact);
    });

    println!("ONCE PER PROCESS (precomputable to disk)");
    println!("  load + parse {} SKILL.md        {load_ms:>9.2} ms", skills.len());
    println!("  embed all skills (model cold)   {cold_ms:>9.2} ms   ← includes loading the embedder");
    println!("  embed all skills (model warm)   {warm_batch_ms:>9.2} ms");
    println!();
    println!("EVERY TURN (cannot be precomputed — depends on the message)");
    println!("  embed the user's query          {query_ms:>9.2} ms");
    println!("  rank 30 skills by cosine        {rank_ms:>9.2} ms");
    println!("  render the selected block       {render_ms:>9.2} ms");
    println!("  (render all 30, for comparison) {render_all_ms:>9.2} ms");
    println!();
    let per_turn = query_ms + rank_ms + render_ms;
    println!("  per-turn total                  {per_turn:>9.2} ms");
    let block = format_skills_for_prompt_mode(&picked, SkillPromptMode::Compact);
    println!();
    println!("FOR SCALE");
    println!(
        "  the block this produces is ≈{} tokens, which the model",
        block.len() / 4
    );
    println!("  must prefill on every turn. At a measured 37.5 tok/s decode and");
    println!("  the prefill rates seen in the daemon e2e, that is the cost that");
    println!("  dominates — not the {per_turn:.1} ms above.");
}
