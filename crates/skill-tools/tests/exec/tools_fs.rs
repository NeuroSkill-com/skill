// SPDX-License-Identifier: GPL-3.0-only
// Tests for skill-tools/src/exec/tools_fs.rs

use skill_tools::types::LlmToolConfig;
use serde_json::json;
use std::fs;
use std::io::Write;
use tempfile::tempdir;

#[tokio::test]
async fn test_exec_read_file_success() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    fs::write(&file_path, "line1\nline2\nline3").unwrap();
    let args = json!({"path": file_path.to_str().unwrap()});
    let config = LlmToolConfig { strict_path_safety: false, ..Default::default() };
    let result = skill_tools::exec::tools_fs::exec_read_file(&args, &config).await;
    assert!(result["ok"].as_bool().unwrap());
    assert_eq!(result["content"].as_str().unwrap().lines().count(), 3);
}

#[tokio::test]
async fn test_exec_write_file_success() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("write.txt");
    let args = json!({"path": file_path.to_str().unwrap(), "content": "hello world"});
    let config = LlmToolConfig { strict_path_safety: false, ..Default::default() };
    let result = skill_tools::exec::tools_fs::exec_write_file(&args, &config).await;
    assert!(result["ok"].as_bool().unwrap());
    let written = fs::read_to_string(&file_path).unwrap();
    assert_eq!(written, "hello world");
}

#[tokio::test]
async fn test_exec_edit_file_success() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("edit.txt");
    fs::write(&file_path, "foo bar").unwrap();
    let args = json!({"path": file_path.to_str().unwrap(), "old_text": "foo", "new_text": "baz"});
    let config = LlmToolConfig { strict_path_safety: false, ..Default::default() };
    let result = skill_tools::exec::tools_fs::exec_edit_file(&args, &config).await;
    assert!(result["ok"].as_bool().unwrap());
    let edited = fs::read_to_string(&file_path).unwrap();
    assert_eq!(edited, "baz bar");
}

#[tokio::test]
async fn test_exec_search_output_head() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("search.txt");
    fs::write(&file_path, "a\nb\nc\nd\ne").unwrap();
    let args = json!({"path": file_path.to_str().unwrap(), "head": 2});
    let config = LlmToolConfig { strict_path_safety: false, ..Default::default() };
    let result = skill_tools::exec::tools_fs::exec_search_output(&args, &config).await;
    assert!(result["ok"].as_bool().unwrap());
    assert!(result["output"].as_str().unwrap().contains("a"));
}

#[tokio::test]
async fn test_exec_read_file_missing_path() {
    let args = json!({});
    let config = LlmToolConfig { strict_path_safety: false, ..Default::default() };
    let result = skill_tools::exec::tools_fs::exec_read_file(&args, &config).await;
    assert!(!result["ok"].as_bool().unwrap());
    assert!(result["error"].as_str().unwrap().contains("missing path"));
}

#[tokio::test]
async fn test_exec_read_file_offset_beyond_end() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test2.txt");
    fs::write(&file_path, "a\nb\nc").unwrap();
    let args = json!({"path": file_path.to_str().unwrap(), "offset": 10});
    let config = LlmToolConfig { strict_path_safety: false, ..Default::default() };
    let result = skill_tools::exec::tools_fs::exec_read_file(&args, &config).await;
    assert!(!result["ok"].as_bool().unwrap());
    assert!(result["error"].as_str().unwrap().contains("beyond end of file"));
}

#[tokio::test]
async fn test_exec_write_file_missing_path() {
    let args = json!({"content": "foo"});
    let config = LlmToolConfig { strict_path_safety: false, ..Default::default() };
    let result = skill_tools::exec::tools_fs::exec_write_file(&args, &config).await;
    assert!(!result["ok"].as_bool().unwrap());
    assert!(result["error"].as_str().unwrap().contains("missing path"));
}

#[tokio::test]
async fn test_exec_edit_file_missing_args() {
    let args = json!({"path": "foo.txt"});
    let config = LlmToolConfig { strict_path_safety: false, ..Default::default() };
    let result = skill_tools::exec::tools_fs::exec_edit_file(&args, &config).await;
    assert!(!result["ok"].as_bool().unwrap());
    assert!(result["error"].as_str().unwrap().contains("missing old_text"));
}

#[tokio::test]
async fn test_exec_edit_file_no_match() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("edit2.txt");
    fs::write(&file_path, "foo bar").unwrap();
    let args = json!({"path": file_path.to_str().unwrap(), "old_text": "baz", "new_text": "qux"});
    let config = LlmToolConfig { strict_path_safety: false, ..Default::default() };
    let result = skill_tools::exec::tools_fs::exec_edit_file(&args, &config).await;
    assert!(!result["ok"].as_bool().unwrap());
    assert!(result["error"].as_str().unwrap().contains("must match exactly"));
}

#[tokio::test]
async fn test_exec_edit_file_multiple_matches() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("edit3.txt");
    fs::write(&file_path, "foo foo bar").unwrap();
    let args = json!({"path": file_path.to_str().unwrap(), "old_text": "foo", "new_text": "baz"});
    let config = LlmToolConfig { strict_path_safety: false, ..Default::default() };
    let result = skill_tools::exec::tools_fs::exec_edit_file(&args, &config).await;
    assert!(!result["ok"].as_bool().unwrap());
    assert!(result["error"].as_str().unwrap().contains("must be unique"));
}

#[tokio::test]
async fn test_exec_search_output_tail() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("search2.txt");
    fs::write(&file_path, "a\nb\nc\nd\ne").unwrap();
    let args = json!({"path": file_path.to_str().unwrap(), "tail": 2});
    let config = LlmToolConfig { strict_path_safety: false, ..Default::default() };
    let result = skill_tools::exec::tools_fs::exec_search_output(&args, &config).await;
    assert!(result["ok"].as_bool().unwrap());
    assert!(result["output"].as_str().unwrap().contains("e"));
}

#[tokio::test]
async fn test_exec_search_output_range() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("search3.txt");
    fs::write(&file_path, "a\nb\nc\nd\ne").unwrap();
    let args = json!({"path": file_path.to_str().unwrap(), "line_start": 2, "line_end": 4});
    let config = LlmToolConfig { strict_path_safety: false, ..Default::default() };
    let result = skill_tools::exec::tools_fs::exec_search_output(&args, &config).await;
    assert!(result["ok"].as_bool().unwrap());
    assert!(result["output"].as_str().unwrap().contains("b"));
    assert!(result["output"].as_str().unwrap().contains("d"));
}

#[tokio::test]
async fn test_exec_search_output_regex() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("search4.txt");
    fs::write(&file_path, "foo\nbar\nbaz\nqux").unwrap();
    let args = json!({"path": file_path.to_str().unwrap(), "pattern": "ba."});
    let config = LlmToolConfig { strict_path_safety: false, ..Default::default() };
    let result = skill_tools::exec::tools_fs::exec_search_output(&args, &config).await;
    assert!(result["ok"].as_bool().unwrap());
    assert!(result["output"].as_str().unwrap().contains(">    2: bar"));
    assert!(result["output"].as_str().unwrap().contains(">    3: baz"));
}

#[tokio::test]
async fn test_exec_search_output_info() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("search5.txt");
    fs::write(&file_path, "foo\nbar").unwrap();
    let args = json!({"path": file_path.to_str().unwrap()});
    let config = LlmToolConfig { strict_path_safety: false, ..Default::default() };
    let result = skill_tools::exec::tools_fs::exec_search_output(&args, &config).await;
    assert!(result["ok"].as_bool().unwrap());
    assert_eq!(result["mode"], "info");
}
