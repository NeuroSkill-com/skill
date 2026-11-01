// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Which built-in tools a chat request needs, and keeping each round to it.
//!
//! The planning itself is rlx-text's `tool_plan`; this file supplies the
//! phrases that call for each NeuroSkill tool. Small models skip parts of a
//! multi-part request ("check my calendar and tell me the time" called only
//! `date`) and search before they know the user's city ("news about my city"
//! searched for those words), so the orchestrator:
//!
//! - adds the plan to the system prompt,
//! - runs planned calls the model skipped alongside the ones it made, and
//! - holds back a call that needs another tool's result until that result is
//!   in, then asks the model to make it.
//!
//! Phrases are deliberately specific: a false match costs an extra tool call,
//! so "what year" (which also matches "what year did WW2 end") is out and
//! "what year is it" is in.

use std::collections::HashSet;

use rlx_text::{DirectAnswer, PlannedCall, PlannedStep, ToolIntent, ToolPlan};
use serde_json::{json, Value};

use crate::tools;

/// Phrases that ask about another place or time, not the user's own.
const ELSEWHERE: &[&str] = &["i'm in", "i am in", "we're in", "we are in", "i live in"];

fn intents() -> Vec<ToolIntent> {
    vec![
        ToolIntent::new(
            "date",
            "the current date and time",
            &[
                "what time",
                "the time",
                "current time",
                "time is it",
                "time now",
                "what day is it",
                "what day is today",
                "which day is it",
                "day of the week",
                "today's date",
                "the date today",
                "date today",
                "what's the date",
                "what is the date",
                "what date is it",
                "what year is it",
                "current year",
                "is it morning",
                "is it afternoon",
                "is it evening",
                "is it night",
            ],
        )
        // About some other moment or clock, not "now" for the user.
        .unless(&[
            "time difference",
            "time zone",
            "timezone",
            "the date of",
            "what time does",
            "what time did",
            "what time do",
            "what time will",
            "what time should",
        ]),
        ToolIntent::new(
            "location",
            "where the user is",
            &[
                "where am i",
                "where are we",
                "where i am",
                "where i'm",
                "my location",
                "my current location",
                "what city am i",
                "which city am i",
                "what city is this",
                "what country am i",
            ],
        )
        .unless(&["where am i supposed", "where am i going wrong", "where am i wrong"]),
        ToolIntent::new(
            "skill",
            "the user's calendar",
            &[
                "my calendar",
                "calendar for today",
                "calendar this week",
                "my meetings",
                "my meeting",
                "any meetings",
                "next meeting",
                "meetings today",
                "meetings this week",
                "my schedule",
                "on my schedule",
                "my agenda",
                "my appointments",
                "my next appointment",
                "my events",
                "upcoming events",
                "am i free",
                "am i busy",
            ],
        )
        .with_arguments(json!({ "command": "calendar_events" })),
        ToolIntent::new(
            "skill",
            "the user's brain state and EEG device",
            &[
                "brain state",
                "my brain",
                "my focus",
                "focus level",
                "how focused",
                "am i focused",
                "am i stressed",
                "am i relaxed",
                "am i calm",
                "my eeg",
                "eeg device",
                "eeg status",
                "device status",
                "my headset",
                "how do i feel",
                "how am i feeling",
                "my stress",
                "stress level",
                "my mood",
                "my attention",
            ],
        )
        .with_arguments(json!({ "command": "status" })),
        ToolIntent::new(
            "skill",
            "the user's health data",
            &[
                "my heart rate",
                "heart rate today",
                "resting heart rate",
                "my hrv",
                "my steps",
                "step count",
                "steps today",
            ],
        )
        .with_arguments(json!({ "command": "health_summary" })),
        ToolIntent::new(
            "web_search",
            "a search about where the user is (use the city from location)",
            &[
                "my city",
                "my town",
                "my area",
                "near me",
                "nearby",
                "around me",
                "local news",
                "local weather",
                "weather here",
                "weather there",
                "weather outside",
                "weather where i am",
                "weather like here",
                "what's the weather",
                "how's the weather",
                "weather today",
                "is it raining",
                "will it rain",
                "going to rain",
                "temperature outside",
                "the forecast",
            ],
        )
        .after("location")
        // A named place: search it directly, the user's location is not needed.
        .unless(ELSEWHERE)
        .unless(&[
            "weather in",
            "weather like in",
            "weather for",
            "weather at",
            "raining in",
            "rain in",
            "forecast for",
            "forecast in",
        ]),
    ]
}

/// Questions the model answers from what it knows. A web search for one of
/// these (Qwen3.5-0.8B searched "good morning translate to spanish" and "25%
/// of 200") costs a round and a network call and adds nothing.
// Translations are not on this list: without a search Qwen3.5-0.8B turned
// "good morning" into "Buena mañana"; with one it found "Buenos días".
fn direct_answers() -> Vec<DirectAnswer> {
    // `location` too: "what's the capital of Italy?" made Qwen3.5-0.8B look
    // up where the user is. Only unplanned calls are dropped, so "the capital
    // of Italy, and where am I?" still gets its location call.
    const WEB: &[&str] = &["web_search", "web_fetch", "location"];
    // Wording that asks for a search after all. Kept to explicit asks:
    // "today" or "now" would cancel the rule for every request that also
    // asks the time.
    const FRESH: &[&str] = &["search", "look up", "look it up", "google", "latest", "recent", "news"];
    vec![
        DirectAnswer::new(
            "arithmetic",
            &[
                "#n plus #n",
                "#n minus #n",
                "#n times #n",
                "#n divided by #n",
                "#n percent of #n",
                "square root of",
            ],
            WEB,
        )
        .unless(FRESH),
        DirectAnswer::new(
            "general knowledge",
            &[
                "who wrote",
                "who painted",
                "who composed",
                "who invented",
                "who discovered",
                "capital of",
                "define",
                "definition of",
                "meaning of",
            ],
            WEB,
        )
        .unless(FRESH),
    ]
}

/// The plan for `request`, limited to the tools offered this turn.
pub(super) fn plan_for(request: &str, offered: &[tools::Tool]) -> ToolPlan {
    let offered: HashSet<&str> = offered.iter().map(|t| t.function.name.as_str()).collect();
    let usable: Vec<ToolIntent> = intents()
        .into_iter()
        .filter(|i| offered.contains(i.tool.as_str()) && i.requires.iter().all(|r| offered.contains(r.as_str())))
        .collect();
    rlx_text::tool_plan::plan_with(request, &usable, &direct_answers())
}

/// The call as the planner compares it: parsed arguments, and a `skill`
/// command alias ("calendar") resolved to the command it runs.
fn as_planned(name: &str, arguments: &str) -> PlannedCall {
    let mut args: Value = serde_json::from_str(arguments).unwrap_or_else(|_| json!({}));
    if name == "skill" {
        let resolved = args
            .get("command")
            .and_then(Value::as_str)
            .and_then(skill_tools::defs::resolve_skill_alias);
        if let Some(cmd) = resolved {
            args["command"] = Value::from(cmd);
        }
    }
    PlannedCall::new(name, args)
}

fn planned_call(step: &PlannedStep, n: usize) -> tools::ToolCall {
    tools::ToolCall {
        id: format!("plan_{n}"),
        call_type: "function".into(),
        function: tools::ToolCallFunction {
            name: step.tool.clone(),
            arguments: step.arguments.to_string(),
        },
    }
}

/// What [`review_round`] changed.
#[derive(Debug, Default)]
pub(super) struct RoundChanges {
    /// Steps still waiting on a result; see [`follow_up`].
    pub waiting: Vec<PlannedStep>,
    /// Calls were dropped for a direct-answer part; see [`direct_note`].
    pub dropped: bool,
}

/// Bring this round's calls in line with the plan.
///
/// Removes calls that need a result not in yet, drops searches for parts the
/// model should answer itself (only while `may_drop`: one reminder per
/// request, after which the model's search goes through), and adds planned
/// calls the model skipped.
pub(super) fn review_round(
    plan: &ToolPlan,
    calls: &mut Vec<tools::ToolCall>,
    executed: &HashSet<(String, String)>,
    may_drop: bool,
) -> RoundChanges {
    let done: Vec<PlannedCall> = executed.iter().map(|(n, a)| as_planned(n, a)).collect();
    let round: Vec<PlannedCall> = calls
        .iter()
        .map(|c| as_planned(&c.function.name, &c.function.arguments))
        .collect();
    let mut review = plan.review_round(&done, &round);
    if !may_drop {
        review.drop.clear();
    }
    if review.is_empty() {
        return RoundChanges::default();
    }
    for i in review.removals() {
        let held = calls.remove(i);
        if review.drop.contains(&i) {
            log::info!(
                "[tool-plan] dropping {}({}): answerable without tools",
                held.function.name,
                held.function.arguments
            );
        } else {
            log::info!(
                "[tool-plan] holding back {}({}) until its input has run",
                held.function.name,
                held.function.arguments
            );
        }
    }
    for (n, step) in review.add.iter().enumerate() {
        log::info!(
            "[tool-plan] adding skipped {}({}) for {} (\"{}\")",
            step.tool,
            step.arguments,
            step.purpose,
            step.matched
        );
        calls.push(planned_call(step, n));
    }
    RoundChanges {
        waiting: review.waiting,
        dropped: !review.drop.is_empty(),
    }
}

/// What to tell the model after a round that left steps waiting.
pub(super) fn follow_up(plan: &ToolPlan, waiting: &[PlannedStep]) -> Option<String> {
    plan.follow_up(waiting)
}

/// What to tell the model after its searches were dropped.
pub(super) fn direct_note(plan: &ToolPlan) -> Option<String> {
    plan.direct_note()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offered(names: &[&str]) -> Vec<tools::Tool> {
        let all = skill_tools::defs::builtin_llm_tools()
            .into_iter()
            .chain(std::iter::once(skill_tools::defs::skill_api_tool()));
        all.filter(|t| names.contains(&t.function.name.as_str())).collect()
    }

    pub(super) fn call(name: &str, args: Value) -> tools::ToolCall {
        tools::ToolCall {
            id: "c".into(),
            call_type: "function".into(),
            function: tools::ToolCallFunction {
                name: name.into(),
                arguments: args.to_string(),
            },
        }
    }

    const DEFAULT: &[&str] = &["date", "location", "web_search", "web_fetch", "skill"];

    #[test]
    fn plans_the_measured_misses() {
        let tools = offered(DEFAULT);
        let names = |q: &str| -> Vec<String> {
            plan_for(q, &tools)
                .steps
                .iter()
                .map(|s| format!("{}{}", s.tool, s.arguments))
                .collect()
        };
        assert_eq!(
            names("Check my calendar for today and tell me the current time."),
            ["skill{\"command\":\"calendar_events\"}", "date{}"]
        );
        assert_eq!(
            names("What's my current focus level, and what day of the week is it?"),
            ["skill{\"command\":\"status\"}", "date{}"]
        );
        assert_eq!(names("Search for news about my city."), ["location{}", "web_search{}"]);
        assert_eq!(
            names("What time is it, where am I, and what's my brain state?"),
            ["date{}", "location{}", "skill{\"command\":\"status\"}"]
        );
        for knowledge in [
            "Who wrote Pride and Prejudice?",
            "What is the capital of France? Answer in one word.",
            "Translate 'good morning' into Spanish.",
            "What year did the Second World War end?",
            "Look up the weather in Paris.",
        ] {
            assert!(names(knowledge).is_empty(), "{knowledge}: {:?}", names(knowledge));
        }
    }

    #[test]
    fn only_offered_tools_are_planned() {
        let tools = offered(&["date", "web_search"]);
        // `location` is off, so a search that needs it is not planned either.
        assert!(plan_for("news about my city", &tools).is_empty());
        assert_eq!(plan_for("what time is it?", &tools).steps.len(), 1);
    }

    #[test]
    fn review_adds_the_skipped_calendar_call() {
        let tools = offered(DEFAULT);
        let plan = plan_for("Check my calendar for today and tell me the current time.", &tools);
        let mut calls = vec![call("date", json!({}))];
        let waiting = review_round(&plan, &mut calls, &HashSet::new(), true).waiting;
        assert!(waiting.is_empty());
        let names: Vec<_> = calls
            .iter()
            .map(|c| (c.function.name.as_str(), c.function.arguments.as_str()))
            .collect();
        assert_eq!(names, [("date", "{}"), ("skill", r#"{"command":"calendar_events"}"#)]);
    }

    #[test]
    fn alias_counts_as_the_planned_command() {
        let tools = offered(DEFAULT);
        let plan = plan_for("what's on my calendar?", &tools);
        let mut calls = vec![call("skill", json!({"command": "neuroskill-calendar"}))];
        assert!(review_round(&plan, &mut calls, &HashSet::new(), true)
            .waiting
            .is_empty());
        assert_eq!(calls.len(), 1);
    }

    #[test]
    fn search_waits_for_the_location() {
        let tools = offered(DEFAULT);
        let plan = plan_for("Search for news about my city.", &tools);
        let mut calls = vec![call("web_search", json!({"query": "news about my city"}))];
        let waiting = review_round(&plan, &mut calls, &HashSet::new(), true).waiting;
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "location");
        let next = follow_up(&plan, &waiting).expect("follow-up");
        assert!(next.contains("web_search"), "{next}");

        let executed: HashSet<_> = [("location".to_string(), "{}".to_string())].into();
        let mut calls = vec![call("web_search", json!({"query": "news San Jose"}))];
        assert!(review_round(&plan, &mut calls, &executed, true).waiting.is_empty());
        assert_eq!(calls.len(), 1);
    }
}

#[cfg(test)]
mod edge_cases {
    use super::*;

    /// `tool{args}` per planned step, then `direct:<purpose>` per direct part.
    fn summary(q: &str) -> Vec<String> {
        let p = plan_for(q, &tests_support::offered_default());
        p.steps
            .iter()
            .map(|s| format!("{}{}", s.tool, s.arguments))
            .chain(p.direct.iter().map(|(purpose, _)| format!("direct:{purpose}")))
            .collect()
    }

    const CAL: &str = r#"skill{"command":"calendar_events"}"#;
    const STATUS: &str = r#"skill{"command":"status"}"#;
    const HEALTH: &str = r#"skill{"command":"health_summary"}"#;

    #[test]
    fn phrasings_plan_what_they_ask_for() {
        let cases: &[(&str, &[&str])] = &[
            // Negated or about somewhere else: no tool.
            ("Don't check my calendar, just tell me the time.", &["date{}"]),
            ("Without looking at my calendar, what's 2+2?", &["direct:arithmetic"]),
            ("I'm in Paris, what's the weather here?", &[]),
            ("What's the weather like in Tokyo?", &[]),
            ("What's the time difference between London and Tokyo?", &[]),
            ("What time does the sun set in Paris?", &[]),
            ("what's the date of Easter 2027?", &[]),
            ("Where am I supposed to put this file?", &[]),
            ("Tell me about the history of the calendar.", &[]),
            // Indirect wording for the user's own data.
            ("Am I free at 3pm?", &[CAL]),
            ("Am I busy today?", &[CAL]),
            ("When is my next meeting?", &[CAL]),
            ("Am I stressed?", &[STATUS]),
            ("How focused am I?", &[STATUS]),
            ("What's my heart rate?", &[HEALTH]),
            ("Where are we?", &["location{}"]),
            ("whats the time", &["date{}"]),
            ("what TIME is it", &["date{}"]),
            // Local weather and searches near the user need the location first.
            ("What's the weather?", &["location{}", "web_search{}"]),
            ("How's the weather today?", &["location{}", "web_search{}"]),
            ("Is it raining here?", &["location{}", "web_search{}"]),
            ("Any restaurants near me?", &["location{}", "web_search{}"]),
            // Answerable without tools.
            ("Translate 'good morning' into Spanish.", &[]),
            (
                "What's 25% of 200, and what year is it?",
                &["date{}", "direct:arithmetic"],
            ),
            (
                "What's the date today? Also, who painted the Mona Lisa?",
                &["date{}", "direct:general knowledge"],
            ),
            // ...unless a search is asked for.
            ("Search who painted the Mona Lisa", &[]),
            ("Look up the latest news about the capital of France", &[]),
        ];
        let mut bad = Vec::new();
        for (q, want) in cases {
            let got = summary(q);
            if got != *want {
                bad.push(format!("{q:?}: got {got:?}, want {want:?}"));
            }
        }
        assert!(bad.is_empty(), "\n{}", bad.join("\n"));
    }

    #[test]
    fn unplanned_location_is_dropped_for_a_knowledge_question() {
        let tools = tests_support::offered_default();
        let plan = plan_for("What time is it, and what's the capital of Italy?", &tools);
        let mut calls = vec![tests::call("location", json!({})), tests::call("date", json!({}))];
        assert!(review_round(&plan, &mut calls, &HashSet::new(), true).dropped);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "date");
        // Asked for, it stays.
        let plan = plan_for("What's the capital of Italy, and where am I?", &tools);
        let mut calls = vec![tests::call("location", json!({}))];
        assert!(!review_round(&plan, &mut calls, &HashSet::new(), true).dropped);
        assert_eq!(calls.len(), 1);
    }

    #[test]
    fn a_dropped_search_is_dropped_once() {
        let tools = tests_support::offered_default();
        let plan = plan_for("Who painted the Mona Lisa?", &tools);
        let search = || tests::call("web_search", json!({"query": "who painted the Mona Lisa"}));
        let mut calls = vec![search()];
        assert!(review_round(&plan, &mut calls, &HashSet::new(), true).dropped);
        assert!(calls.is_empty());
        assert!(direct_note(&plan).unwrap().contains("general knowledge"));
        // Second attempt after the reminder: let it through.
        let mut calls = vec![search()];
        assert!(!review_round(&plan, &mut calls, &HashSet::new(), false).dropped);
        assert_eq!(calls.len(), 1);
    }
}

#[cfg(test)]
mod tests_support {
    pub(super) fn offered_default() -> Vec<crate::tools::Tool> {
        let names = ["date", "location", "web_search", "web_fetch", "skill"];
        skill_tools::defs::builtin_llm_tools()
            .into_iter()
            .chain(std::iter::once(skill_tools::defs::skill_api_tool()))
            .filter(|t| names.contains(&t.function.name.as_str()))
            .collect()
    }
}
