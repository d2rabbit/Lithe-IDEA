//! Shared-core bridge for the Linux native client.
//!
//! Mirrors the Windows host's `platform_invoke` contract: every product
//! behavior goes through `lithe_core::execute_json` with the same command
//! names, envelopes, and error shapes. No product behavior is implemented in
//! this binary.

use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

/// Executes one shared-core command and returns its `data` payload.
pub fn execute(command: &str, payload: Value) -> Result<Value, String> {
    let id = format!(
        "linux-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or_default()
    );
    let request = serde_json::json!({
        "id": id,
        "operationId": id,
        "command": command,
        "payload": payload,
    })
    .to_string();

    let response = lithe_core::execute_json(&request);
    let envelope: Value = serde_json::from_str(&response)
        .map_err(|error| format!("Shared core returned invalid JSON: {error}"))?;

    if envelope.get("ok").and_then(Value::as_bool) == Some(true) {
        return Ok(envelope.get("data").cloned().unwrap_or(Value::Null));
    }

    let message = envelope
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or("Shared core operation failed");
    Err(message.to_string())
}

/// Lists workspace-relative file paths for one opened workspace root.
pub fn workspace_files(root: &str) -> Result<Vec<String>, String> {
    let data = execute("workspace.snapshot", serde_json::json!({ "root": root }))?;
    let files = data
        .get("files")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default();
    Ok(files)
}

/// Reads one workspace-relative UTF-8 file.
pub fn read_file(root: &str, path: &str) -> Result<String, String> {
    let data = execute(
        "file.read",
        serde_json::json!({ "root": root, "path": path }),
    )?;
    data.get("text")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| "File read returned no text".to_string())
}

/// Writes one workspace-relative UTF-8 file.
pub fn write_file(root: &str, path: &str, text: &str) -> Result<(), String> {
    execute(
        "file.write",
        serde_json::json!({ "root": root, "path": path, "text": text }),
    )
    .map(|_| ())
}

/// Reads the current Git branch for one workspace root, when available.
pub fn git_branch(root: &str) -> Option<String> {
    let data = execute("git.status", serde_json::json!({ "root": root })).ok()?;
    data.get("branch")
        .and_then(Value::as_str)
        .filter(|branch| !branch.is_empty())
        .map(ToString::to_string)
}
