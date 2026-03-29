// SPDX-License-Identifier: GPL-3.0-only
//! Tests for schema-driven type coercion of LLM tool-call arguments.

use serde_json::json;
use skill_tools::parse::coerce_value;

fn schema(ty: &str) -> serde_json::Value {
    json!({ "type": ty })
}

// ── Boolean coercion ─────────────────────────────────────────────────────────

#[test]
fn string_true_to_bool() {
    assert_eq!(coerce_value(&json!("true"), &schema("boolean")), json!(true));
}

#[test]
fn string_false_to_bool() {
    assert_eq!(coerce_value(&json!("false"), &schema("boolean")), json!(false));
}

#[test]
fn string_yes_to_bool() {
    assert_eq!(coerce_value(&json!("yes"), &schema("boolean")), json!(true));
}

#[test]
fn number_1_to_bool() {
    assert_eq!(coerce_value(&json!(1), &schema("boolean")), json!(true));
}

#[test]
fn number_0_to_bool() {
    assert_eq!(coerce_value(&json!(0), &schema("boolean")), json!(false));
}

#[test]
fn bool_unchanged() {
    assert_eq!(coerce_value(&json!(true), &schema("boolean")), json!(true));
}

// ── Number coercion ──────────────────────────────────────────────────────────

#[test]
fn string_int_to_number() {
    assert_eq!(coerce_value(&json!("42"), &schema("number")), json!(42.0));
}

#[test]
fn string_float_to_number() {
    assert_eq!(coerce_value(&json!("3.14"), &schema("number")), json!(3.14));
}

#[test]
fn string_int_to_integer() {
    assert_eq!(coerce_value(&json!("7"), &schema("integer")), json!(7));
}

#[test]
fn float_to_integer_truncates() {
    // 5.0 should become integer 5
    assert_eq!(coerce_value(&json!(5.0), &schema("integer")), json!(5));
}

#[test]
fn bool_to_number() {
    assert_eq!(coerce_value(&json!(true), &schema("number")), json!(1));
}

// ── String coercion ──────────────────────────────────────────────────────────

#[test]
fn number_to_string() {
    assert_eq!(coerce_value(&json!(42), &schema("string")), json!("42"));
}

#[test]
fn bool_to_string() {
    assert_eq!(coerce_value(&json!(false), &schema("string")), json!("false"));
}

#[test]
fn string_unchanged() {
    assert_eq!(coerce_value(&json!("hello"), &schema("string")), json!("hello"));
}

// ── Null coercion ────────────────────────────────────────────────────────────

#[test]
fn string_null_to_null() {
    assert_eq!(coerce_value(&json!("null"), &schema("null")), json!(null));
}

#[test]
fn empty_string_to_null() {
    assert_eq!(coerce_value(&json!(""), &schema("null")), json!(null));
}

// ── Object coercion ──────────────────────────────────────────────────────────

#[test]
fn string_json_to_object() {
    let schema = json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" }
        }
    });
    let value = json!(r#"{"name": "test"}"#);
    let result = coerce_value(&value, &schema);
    assert_eq!(result, json!({"name": "test"}));
}

#[test]
fn nested_property_coercion() {
    let schema = json!({
        "type": "object",
        "properties": {
            "count": { "type": "integer" },
            "enabled": { "type": "boolean" }
        }
    });
    let value = json!({"count": "5", "enabled": "true"});
    let result = coerce_value(&value, &schema);
    assert_eq!(result["count"], json!(5));
    assert_eq!(result["enabled"], json!(true));
}

// ── Array coercion ───────────────────────────────────────────────────────────

#[test]
fn string_json_to_array() {
    let schema = json!({
        "type": "array",
        "items": { "type": "integer" }
    });
    let value = json!("[1, 2, 3]");
    let result = coerce_value(&value, &schema);
    assert_eq!(result, json!([1, 2, 3]));
}

#[test]
fn array_items_coerced() {
    let schema = json!({
        "type": "array",
        "items": { "type": "boolean" }
    });
    let value = json!(["true", "false", "1"]);
    let result = coerce_value(&value, &schema);
    assert_eq!(result, json!([true, false, true]));
}

// ── No coercion needed ──────────────────────────────────────────────────────

#[test]
fn passthrough_when_no_schema() {
    // Boolean schema (true) = accept all
    assert_eq!(coerce_value(&json!(42), &json!(true)), json!(42));
}

#[test]
fn passthrough_matching_type() {
    assert_eq!(coerce_value(&json!(42), &schema("number")), json!(42));
}
