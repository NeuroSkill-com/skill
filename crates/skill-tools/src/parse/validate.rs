// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! JSON Schema validation of LLM tool-call arguments.

use serde_json::Value;
use super::types::Tool;
use super::coerce::coerce_value;

/// Validate tool-call arguments against the tool's JSON Schema `parameters`.
///
/// Returns the (potentially coerced) arguments value, or an `Err` with a
/// human-readable validation error message.
///
/// Before validation the arguments are **coerced** to match the schema types.
/// Different LLM backends (Llama, Qwen, Mistral, Gemma, DeepSeek, …) emit
/// arguments in subtly different formats — e.g. `"true"` instead of `true`,
/// `"3"` instead of `3`, or a bare string instead of an object.  The coercion
/// step normalises these so the downstream validation and execution always see
/// correct types.
pub fn validate_tool_arguments(tool: &Tool, args: &Value) -> Result<Value, String> {
    let Some(ref schema) = tool.function.parameters else {
        // No schema defined — accept any arguments.
        return Ok(args.clone());
    };

    // Coerce arguments to match schema-declared types.
    let coerced = coerce_value(args, schema);

    let compiled = jsonschema::validator_for(schema)
        .map_err(|e| format!("Invalid tool schema for \"{}\": {e}", tool.function.name))?;

    let errors: Vec<String> = compiled
        .iter_errors(&coerced)
        .map(|err| {
            let path_str = err.instance_path.to_string();
            let path = if path_str.is_empty() {
                "root".to_string()
            } else {
                path_str
            };
            format!("  - {path}: {err}")
        })
        .collect();

    if !errors.is_empty() {
        return Err(format!(
            "Validation failed for tool \"{}\":\n{}\n\nReceived arguments:\n{}",
            tool.function.name,
            errors.join("\n"),
            serde_json::to_string_pretty(&coerced).unwrap_or_default()
        ));
    }

    Ok(coerced)
}
