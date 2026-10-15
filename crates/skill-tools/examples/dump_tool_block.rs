// SPDX-License-Identifier: GPL-3.0-only
//! Print the tool block the daemon injects — the *stable* half of the system
//! prompt, and therefore the half a prefix KV cache could reuse.
fn main() {
    let tools = skill_tools::defs::builtin_llm_tools();
    let mut messages = vec![serde_json::json!({"role": "system", "content": ""})];
    skill_tools::parse::inject_tools_into_system_prompt(&mut messages, &tools, 8192);
    print!("{}", messages[0]["content"].as_str().unwrap_or_default());
}
