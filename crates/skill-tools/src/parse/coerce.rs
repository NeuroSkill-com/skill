// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Schema-driven type coercion for LLM tool-call arguments.
//!
//! Different LLM backends (Llama, Qwen, Mistral, Gemma, DeepSeek, …) emit
//! arguments in subtly different formats — e.g. `"true"` instead of `true`,
//! `"3"` instead of `3`, or a bare string instead of an object.  The coercion
//! functions here normalise these so downstream validation and execution always
//! see correct types.

use serde_json::Value;
use super::types::{Tool, ToolCall};

/// Recursively coerce `value` to match the types declared in `schema`.
///
/// Handles the most common multi-model mismatches:
///  - `"true"` / `"false"` → `bool`    (when schema says `"type": "boolean"`)
///  - `"123"` / `"3.14"`  → `number`   (when schema says `"type": "number"` / `"integer"`)
///  - `42`                → `"42"`      (when schema says `"type": "string"`)
///  - `"null"` / `""`     → `null`     (when schema says `"type": "null"` or field is nullable)
///  - string-encoded JSON → parsed     (when schema expects object/array and value is a string)
///  - object properties   → recurse    (each property coerced against its own sub-schema)
///  - `null` for missing optional fields is passed through unchanged
pub(crate) fn coerce_value(value: &Value, schema: &Value) -> Value {
    // If schema is a boolean schema (`true` = accept all, `false` = reject all)
    // or not an object, return value as-is.
    let Some(schema_obj) = schema.as_object() else {
        return value.clone();
    };

    // Resolve the target type(s) declared by the schema.
    let target_types = schema_type_set(schema_obj);

    // Handle `oneOf` / `anyOf` — try each sub-schema and pick the first that
    // succeeds validation after coercion.
    for key in &["oneOf", "anyOf"] {
        if let Some(arr) = schema_obj.get(*key).and_then(|v| v.as_array()) {
            for sub in arr {
                let coerced = coerce_value(value, sub);
                if let Ok(compiled) = jsonschema::validator_for(sub) {
                    if compiled.iter_errors(&coerced).next().is_none() {
                        return coerced;
                    }
                }
            }
        }
    }

    // Object coercion: recurse into properties.
    if target_types.contains(&"object") || (target_types.is_empty() && value.is_object()) {
        return coerce_object(value, schema_obj);
    }

    // Array coercion: if schema expects array and value is a JSON-encoded string.
    if target_types.contains(&"array") {
        return coerce_array(value, schema_obj);
    }

    // Scalar coercion based on target type.
    if target_types.contains(&"boolean") {
        if let Some(b) = coerce_to_bool(value) {
            return Value::Bool(b);
        }
    }

    if target_types.contains(&"number") || target_types.contains(&"integer") {
        if let Some(n) = coerce_to_number(value, target_types.contains(&"integer")) {
            return n;
        }
    }

    if target_types.contains(&"string") {
        if let Some(s) = coerce_to_string(value) {
            return Value::String(s);
        }
    }

    if target_types.contains(&"null") {
        if let Some(s) = value.as_str() {
            let lower = s.trim().to_ascii_lowercase();
            if lower == "null" || lower.is_empty() {
                return Value::Null;
            }
        }
    }

    // No coercion applicable — return as-is.
    value.clone()
}

/// Coerce a [`ToolCall`]'s arguments string in-place against a matching tool
/// definition.  This is useful in the execution layer to normalise arguments
/// *before* they are parsed into typed structs.
///
/// Returns the coerced arguments as a parsed [`Value`].
pub fn coerce_tool_call_arguments(call: &mut ToolCall, tools: &[Tool]) -> Value {
    let args: Value = serde_json::from_str(&call.function.arguments)
        .unwrap_or_else(|_| serde_json::json!({}));

    let Some(tool) = tools.iter().find(|t| t.function.name == call.function.name) else {
        return args;
    };
    let Some(ref schema) = tool.function.parameters else {
        return args;
    };

    let coerced = coerce_value(&args, schema);
    call.function.arguments = coerced.to_string();
    coerced
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn coerce_object(value: &Value, schema_obj: &serde_json::Map<String, Value>) -> Value {
    // If the value is a string that looks like JSON, try to parse it first.
    if let Some(s) = value.as_str() {
        let trimmed = s.trim();
        if trimmed.starts_with('{') {
            if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
                return coerce_value(&parsed, &Value::Object(schema_obj.clone()));
            }
        }
    }

    let Some(obj) = value.as_object() else {
        return value.clone();
    };

    let props = schema_obj.get("properties").and_then(|p| p.as_object());
    let no_additional = schema_obj.get("additionalProperties")
        .and_then(|v| v.as_bool()) == Some(false);

    // When the schema forbids additional properties and has an `args`
    // property of type object, collect any unknown top-level keys
    // into `args`.  This handles LLMs that flatten command arguments
    // to the top level instead of nesting them under `args`.
    let has_args_prop = props.is_some_and(|p| p.contains_key("args"));

    if no_additional && has_args_prop {
        return coerce_object_with_args_folding(obj, props, schema_obj);
    }

    let mut out = serde_json::Map::new();
    for (k, v) in obj {
        if let Some(prop_schema) = props.and_then(|p| p.get(k)) {
            out.insert(k.clone(), coerce_value(v, prop_schema));
        } else {
            out.insert(k.clone(), v.clone());
        }
    }
    Value::Object(out)
}

/// Handle the special case where unknown top-level keys get folded into an
/// `args` sub-object (for the `skill` tool).
fn coerce_object_with_args_folding(
    obj: &serde_json::Map<String, Value>,
    props: Option<&serde_json::Map<String, Value>>,
    schema_obj: &serde_json::Map<String, Value>,
) -> Value {
    let mut out = serde_json::Map::new();
    let mut extra = serde_json::Map::new();

    // Collect existing nested args — accept both "args" and
    // "arguments" (common LLM alias).
    let existing_args = obj.get("args")
        .or_else(|| obj.get("arguments"))
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    for (k, v) in obj {
        if k == "args" || k == "arguments" {
            // Will be merged below.
            continue;
        }
        if props.is_some_and(|p| p.contains_key(k)) {
            // Safe: we just checked props contains k.
            let prop_schema = props.and_then(|p| p.get(k));
            if let Some(ps) = prop_schema {
                out.insert(k.clone(), coerce_value(v, ps));
            } else {
                out.insert(k.clone(), v.clone());
            }
        } else {
            extra.insert(k.clone(), v.clone());
        }
    }

    // Merge: existing args take precedence over flattened extras.
    if !extra.is_empty() || !existing_args.is_empty() {
        let mut merged = extra;
        for (k, v) in existing_args {
            merged.insert(k, v);
        }
        let args_schema = schema_obj.get("properties")
            .and_then(|p| p.get("args"))
            .cloned()
            .unwrap_or(Value::Bool(true));
        out.insert("args".to_string(), coerce_value(&Value::Object(merged), &args_schema));
    }

    Value::Object(out)
}

fn coerce_array(value: &Value, schema_obj: &serde_json::Map<String, Value>) -> Value {
    // If the value is a string that looks like a JSON array, try to parse it.
    if let Some(s) = value.as_str() {
        let trimmed = s.trim();
        if trimmed.starts_with('[') {
            if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
                if let Some(arr) = parsed.as_array() {
                    let items_schema = schema_obj.get("items").cloned()
                        .unwrap_or(Value::Bool(true));
                    let coerced: Vec<Value> = arr.iter()
                        .map(|item| coerce_value(item, &items_schema))
                        .collect();
                    return Value::Array(coerced);
                }
            }
        }
    }
    if let Some(arr) = value.as_array() {
        let items_schema = schema_obj.get("items").cloned()
            .unwrap_or(Value::Bool(true));
        let coerced: Vec<Value> = arr.iter()
            .map(|item| coerce_value(item, &items_schema))
            .collect();
        return Value::Array(coerced);
    }
    value.clone()
}

/// Extract the set of type names from a schema object.
/// Handles `"type": "string"` and `"type": ["string", "null"]`.
fn schema_type_set<'a>(schema: &'a serde_json::Map<String, Value>) -> Vec<&'a str> {
    match schema.get("type") {
        Some(Value::String(s)) => vec![s.as_str()],
        Some(Value::Array(arr)) => arr.iter().filter_map(|v| v.as_str()).collect(),
        _ => vec![],
    }
}

/// Try to coerce a value to a boolean.
fn coerce_to_bool(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(b) => Some(*b),
        Value::String(s) => match s.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Some(true),
            "false" | "0" | "no" | "off" => Some(false),
            _ => None,
        },
        Value::Number(n) => n.as_i64().map(|i| i != 0).or_else(|| n.as_f64().map(|f| f != 0.0)),
        _ => None,
    }
}

/// Try to coerce a value to a JSON number.
fn coerce_to_number(value: &Value, integer_only: bool) -> Option<Value> {
    match value {
        Value::Number(_) => {
            if integer_only {
                // Coerce float to integer if schema requires it.
                if let Some(f) = value.as_f64() {
                    if f.fract() == 0.0 && f >= i64::MIN as f64 && f <= i64::MAX as f64 {
                        return Some(Value::Number(serde_json::Number::from(f as i64)));
                    }
                }
            }
            Some(value.clone())
        }
        Value::String(s) => {
            let trimmed = s.trim();
            if integer_only {
                if let Ok(i) = trimmed.parse::<i64>() {
                    return Some(Value::Number(serde_json::Number::from(i)));
                }
            }
            if let Ok(f) = trimmed.parse::<f64>() {
                if integer_only && f.fract() == 0.0 && f >= i64::MIN as f64 && f <= i64::MAX as f64 {
                    return Some(Value::Number(serde_json::Number::from(f as i64)));
                }
                serde_json::Number::from_f64(f).map(Value::Number)
            } else {
                None
            }
        }
        Value::Bool(b) => Some(Value::Number(serde_json::Number::from(if *b { 1 } else { 0 }))),
        _ => None,
    }
}

/// Try to coerce a value to a string.
fn coerce_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(_) => None, // Already correct type.
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Null => Some(String::new()),
        // Don't coerce objects/arrays to strings — that's almost certainly wrong.
        _ => None,
    }
}
