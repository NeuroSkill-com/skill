// SPDX-License-Identifier: GPL-3.0-only
//! Does the right skill actually get found? Lexical vs semantic, scored.
//!
//! Token count alone says nothing about whether a preset still lets the model
//! reach the right instructions. This scores the *selection* half — whether
//! the correct skill survives the top-N cut — against a labelled query set,
//! and prints the cost of each rendering preset beside it.
//!
//! ```sh
//! cargo run --release -p skill-daemon-state --features text-embeddings-rlx-metal \
//!   --bin eval_skill_retrieval -- skills/skills
//! ```
//!
//! ## On the query set
//!
//! The queries are hand-written and split into two tiers, because the two
//! rankers fail in different places and an average over one tier would hide it:
//!
//! * **easy** — the query reuses a word from the skill's description, which is
//!   the only case word-overlap ranking can serve;
//! * **hard** — a natural paraphrase that deliberately avoids the skill's own
//!   vocabulary, which is how people actually type.
//!
//! They were written by reading the descriptions, so they are not an unbiased
//! sample of real user traffic — treat them as a regression harness and a
//! like-for-like comparison between rankers, not as an absolute accuracy
//! figure.

use skill_daemon_state::text_embedder::SharedTextEmbedder;
use skill_skills::{
    format_skills_for_prompt_mode, load_skills, rank_skills_by_embedding, rerank_skills_for_prompt,
    skill_embedding_text, LoadSkillsOptions, Skill, SkillPromptMode,
};

/// `(query, expected skill, hard?)`
const QUERIES: &[(&str, &str, bool)] = &[
    // ── easy: the query shares vocabulary with the description ──────────────
    ("show me my sleep stages", "neuroskill-sleep", false),
    ("search my screenshots for text", "neuroskill-screenshots", false),
    ("what is my device status", "neuroskill-status", false),
    ("set up do not disturb automation", "neuroskill-dnd", false),
    ("download a gguf model", "neuroskill-llm", false),
    ("fetch my calendar events", "neuroskill-calendar", false),
    ("create an eeg label", "neuroskill-labels", false),
    ("music for focus", "neuroskill-protocols-music", false),
    ("caffeine and fasting guidance", "neuroskill-protocols-nutrition", false),
    ("meditation with biofeedback", "neuroskill-protocols-meditation", false),
    ("per session metric breakdown", "neuroskill-sessions", false),
    ("websocket and http port discovery", "neuroskill-transport", false),
    ("weekly digest of my productivity", "neuroskill-activity", false),
    ("real-time eeg pattern matching hooks", "neuroskill-hooks", false),
    ("what band power fields exist", "neuroskill-data-reference", false),
    // ── hard: natural phrasing, avoids the skill's own words ────────────────
    ("help me wind down before bed", "neuroskill-protocols-sleep", true),
    (
        "i cannot stop picking up my phone",
        "neuroskill-protocols-digital",
        true,
    ),
    ("my neck aches after sitting all day", "neuroskill-protocols-body", true),
    ("i am furious with my colleague", "neuroskill-protocols-emotions", true),
    (
        "my nose is blocked, is there another way",
        "neuroskill-protocols-breathfree",
        true,
    ),
    (
        "what should i eat to avoid the afternoon crash",
        "neuroskill-protocols-nutrition",
        true,
    ),
    (
        "my heart is pounding and i feel wound up",
        "neuroskill-protocols-stress",
        true,
    ),
    (
        "i want to pay attention while i eat",
        "neuroskill-protocols-mindfulness",
        true,
    ),
    (
        "what should i do first thing after waking",
        "neuroskill-protocols-routines",
        true,
    ),
    (
        "my teenager cannot concentrate on homework",
        "neuroskill-protocols-life",
        true,
    ),
    (
        "find pictures from around three in the afternoon",
        "neuroskill-screenshots",
        true,
    ),
    ("read it out to me instead", "neuroskill-streaming", true),
    ("when have i felt like this before", "neuroskill-search", true),
    ("give me a copy-pasteable shell example", "neuroskill-recipes", true),
    ("how do i tell whether this is helping me", "neuroskill-evidence", true),
];

fn hit(ranked: &[Skill], want: &str, k: usize) -> bool {
    ranked.iter().take(k).any(|s| s.name == want)
}

struct Score {
    at1: usize,
    at3: usize,
    at8: usize,
    n: usize,
}

impl Score {
    fn row(&self, label: &str) -> String {
        let pct = |v: usize| 100.0 * v as f64 / self.n.max(1) as f64;
        format!(
            "  {:<22} {:>3}/{:<3} {:>5.0}%   {:>3}/{:<3} {:>5.0}%   {:>3}/{:<3} {:>5.0}%",
            label,
            self.at1,
            self.n,
            pct(self.at1),
            self.at3,
            self.n,
            pct(self.at3),
            self.at8,
            self.n,
            pct(self.at8)
        )
    }
}

fn score<F>(skills: &[Skill], tier: Option<bool>, mut rank: F) -> Score
where
    F: FnMut(&str, Vec<Skill>, usize) -> Vec<Skill>,
{
    let mut s = Score {
        at1: 0,
        at3: 0,
        at8: 0,
        n: 0,
    };
    for (q, want, hard) in QUERIES {
        if tier.is_some_and(|t| t != *hard) {
            continue;
        }
        s.n += 1;
        // Rank once per cut, with `top_n` set to that cut.
        //
        // Not once at `top_n = len`: both rankers short-circuit and return the
        // list *unsorted* when `skills.len() <= top_n`, so asking for the whole
        // catalogue measures catalogue order rather than ranking — which is
        // what made lexical and semantic score identically on the first run.
        if hit(&rank(q, skills.to_vec(), 1), want, 1) {
            s.at1 += 1;
        }
        if hit(&rank(q, skills.to_vec(), 3), want, 3) {
            s.at3 += 1;
        }
        if hit(&rank(q, skills.to_vec(), 8), want, 8) {
            s.at8 += 1;
        }
    }
    s
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "skills/skills".into());
    // `--dump-selected` emits, per query, the block the daemon would actually
    // build: the semantic top-N, rendered compact. The Python preset eval
    // feeds that to a model, so the two halves measure the same pipeline
    // rather than two reconstructions of it.
    let dump = std::env::args().any(|a| a == "--dump-selected");
    let skills = load_skills(&LoadSkillsOptions {
        skill_paths: vec![std::path::PathBuf::from(&dir)],
        include_defaults: false,
        ..Default::default()
    })
    .skills;

    // A mislabelled expectation would silently depress every score.
    let missing: Vec<&str> = QUERIES
        .iter()
        .map(|(_, w, _)| *w)
        .filter(|w| !skills.iter().any(|s| s.name == *w))
        .collect();
    assert!(
        missing.is_empty(),
        "query set names skills that are not loaded: {missing:?}"
    );

    if !dump {
        println!("{} skills from {dir}, {} queries\n", skills.len(), QUERIES.len());
    }

    // ── Selection: does the right skill survive the cut? ────────────────────
    let embedder = SharedTextEmbedder::new();
    let texts: Vec<String> = skills.iter().map(skill_embedding_text).collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    // Skills are the indexed side, the user's message is the query:
    // nomic is asymmetric and wants `search_document:` / `search_query:`.
    // Embedding both sides the same way is the classic way to leave
    // retrieval accuracy on the table.
    let skill_vecs = embedder.embed_documents(refs);
    match &skill_vecs {
        Some(v) if !dump => println!("embedder: loaded, {} dims\n", v.first().map_or(0, Vec::len)),
        None if !dump => println!("embedder: UNAVAILABLE — semantic rows will be omitted\n"),
        _ => {}
    }

    if dump {
        let top_n: usize = std::env::args()
            .skip_while(|a| a != "--top-n")
            .nth(1)
            .and_then(|v| v.parse().ok())
            .unwrap_or(8);
        let vecs = skill_vecs.as_ref().expect("--dump-selected needs the embedder");
        let mut out = serde_json::Map::new();
        for (q, _, _) in QUERIES {
            let pv = embedder.embed_query(q).expect("embed query");
            let picked = rank_skills_by_embedding(q, skills.clone(), &pv, vecs, top_n);
            out.insert(
                (*q).to_string(),
                serde_json::Value::String(format_skills_for_prompt_mode(&picked, SkillPromptMode::Compact)),
            );
        }
        println!("{}", serde_json::Value::Object(out));
        return;
    }

    for (tier_label, tier) in [
        ("ALL", None),
        ("easy (shares words)", Some(false)),
        ("hard (paraphrase)", Some(true)),
    ] {
        println!("{tier_label}");
        println!(
            "  {:<22} {:^14}   {:^14}   {:^14}",
            "ranker", "recall@1", "recall@3", "recall@8"
        );

        let lex = score(&skills, tier, |q, s, n| rerank_skills_for_prompt(q, s, n));
        println!("{}", lex.row("lexical"));

        if let Some(vecs) = &skill_vecs {
            let sem = score(&skills, tier, |q, s, n| {
                let Some(pv) = embedder.embed_query(q) else {
                    return s;
                };
                rank_skills_by_embedding(q, s, &pv, vecs, n)
            });
            println!("{}", sem.row("semantic"));
        }
        println!();
    }

    // ── Cacheability: how much of the prompt repeats turn to turn? ──────────
    //
    // A prompt-prefix KV cache can only reuse a *prefix*. The prompt is built
    // [tool block][user system][skills], so the tool block is reusable on
    // every turn, and the skills block is reusable only when consecutive
    // messages happen to select the same skills — which is why the selected
    // set is rendered in a canonical order rather than in rank order.
    {
        let top_n = 8usize;
        let mut sets: Vec<Vec<String>> = Vec::new();
        for (q, _, _) in QUERIES {
            let picked = match (&skill_vecs, embedder.embed_query(q)) {
                (Some(v), Some(pv)) => rank_skills_by_embedding(q, skills.clone(), &pv, v, top_n),
                _ => rerank_skills_for_prompt(q, skills.clone(), top_n),
            };
            let mut names: Vec<String> = picked.iter().map(|s| s.name.clone()).collect();
            names.sort();
            sets.push(names);
        }
        let unique: std::collections::BTreeSet<&Vec<String>> = sets.iter().collect();
        let repeats = sets.len() - unique.len();

        // How much of a selected block two consecutive queries share.
        let mut shared = 0usize;
        for w in sets.windows(2) {
            let a: std::collections::BTreeSet<&String> = w[0].iter().collect();
            shared += w[1].iter().filter(|n| a.contains(n)).count();
        }
        let mean_shared = shared as f64 / (sets.len().saturating_sub(1).max(1)) as f64;

        println!("cacheability");
        println!(
            "  distinct selected sets        {} of {} queries",
            unique.len(),
            sets.len()
        );
        println!("  exact-repeat selections       {repeats}");
        println!("  skills shared with previous   {mean_shared:.1} of {top_n} on average");
        println!();
    }

    // ── Rendering: what each preset costs ───────────────────────────────────
    //
    // The selecting presets are query-dependent, so they are averaged over the
    // query set rather than measured once.
    println!("preset cost");
    let full = format_skills_for_prompt_mode(&skills, SkillPromptMode::Full).len();
    for m in [
        SkillPromptMode::Full,
        SkillPromptMode::Compact,
        SkillPromptMode::Grouped,
        SkillPromptMode::Brief,
        SkillPromptMode::NamesOnly,
        SkillPromptMode::Smart,
        SkillPromptMode::SmartGrouped,
    ] {
        let (n, note) = if m.selects() {
            let top_n = 8usize;
            let mut total = 0usize;
            for (q, _, _) in QUERIES {
                let picked = match (&skill_vecs, embedder.embed_query(q)) {
                    (Some(v), Some(pv)) => rank_skills_by_embedding(q, skills.clone(), &pv, v, top_n),
                    _ => rerank_skills_for_prompt(q, skills.clone(), top_n),
                };
                total += format_skills_for_prompt_mode(&picked, m).len();
            }
            (
                total / QUERIES.len().max(1),
                format!(" (top-{top_n}, mean over queries)"),
            )
        } else {
            (
                format_skills_for_prompt_mode(&skills, m).len(),
                format!(" (all {} skills)", skills.len()),
            )
        };
        println!(
            "  {:<14} ≈{:>5} tokens   {:>5.1}% of full{note}",
            m.as_str(),
            n / 4,
            100.0 * n as f64 / full.max(1) as f64
        );
    }
}
