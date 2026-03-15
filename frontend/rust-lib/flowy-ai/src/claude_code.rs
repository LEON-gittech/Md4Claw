use flowy_ai_pub::cloud::{
  CompleteTextParams, CompletionStreamValue, CompletionType, StreamComplete,
};
use flowy_error::FlowyError;
use futures::StreamExt;
use futures::stream::BoxStream;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{error, info};

/// Check if the `claude` CLI is available on the system
pub async fn is_claude_code_available() -> bool {
  Command::new("which")
    .arg("claude")
    .output()
    .await
    .map(|output| output.status.success())
    .unwrap_or(false)
}

/// Build a system prompt appropriate for the completion type
fn build_system_prompt(completion_type: &CompletionType) -> String {
  match completion_type {
    CompletionType::ImproveWriting => {
      "You are a writing assistant. Improve the provided text for clarity, flow, and readability. \
       Output ONLY the improved text, nothing else. Do not add explanations or commentary."
        .to_string()
    },
    CompletionType::SpellingAndGrammar => {
      "You are a proofreading assistant. Fix spelling and grammar errors in the provided text. \
       Output ONLY the corrected text, nothing else. Do not add explanations."
        .to_string()
    },
    CompletionType::MakeShorter => {
      "You are a writing assistant. Condense the provided text while preserving key information. \
       Output ONLY the shortened text, nothing else."
        .to_string()
    },
    CompletionType::MakeLonger => {
      "You are a writing assistant. Expand the provided text with additional detail and depth. \
       Output ONLY the expanded text, nothing else."
        .to_string()
    },
    CompletionType::ContinueWriting => {
      "You are a writing assistant. Continue writing from where the text ends, maintaining \
       the same style and tone. Output ONLY the continuation text, nothing else."
        .to_string()
    },
    CompletionType::Explain => {
      "You are a helpful assistant. Explain the provided text in clear, simple terms.".to_string()
    },
    CompletionType::AskAI | CompletionType::CustomPrompt => {
      "You are a helpful AI assistant embedded in a document editor. \
       Answer questions accurately and concisely."
        .to_string()
    },
  }
}

/// Execute a completion request using the Claude CLI
pub async fn stream_complete_with_claude_code(
  params: CompleteTextParams,
) -> Result<StreamComplete, FlowyError> {
  let completion_type = params
    .completion_type
    .as_ref()
    .cloned()
    .unwrap_or(CompletionType::AskAI);

  // Build the system prompt
  let system_prompt = if let Some(ref meta) = params.metadata {
    if let Some(ref custom) = meta.custom_prompt {
      custom.system.clone()
    } else {
      build_system_prompt(&completion_type)
    }
  } else {
    build_system_prompt(&completion_type)
  };

  // Build the user message with full context
  let user_message = build_user_message(&params, &completion_type);

  info!(
    "Claude Code completion: type={:?}, text_len={}",
    completion_type,
    user_message.len()
  );

  // Spawn the claude CLI process
  let mut child = Command::new("claude")
    .args([
      "-p",
      "--output-format",
      "stream-json",
      "--system-prompt",
      &system_prompt,
    ])
    .stdin(std::process::Stdio::piped())
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .spawn()
    .map_err(|e| {
      FlowyError::internal().with_context(format!("Failed to spawn claude CLI: {}", e))
    })?;

  // Write the user message to stdin
  if let Some(mut stdin) = child.stdin.take() {
    use tokio::io::AsyncWriteExt;
    stdin
      .write_all(user_message.as_bytes())
      .await
      .map_err(|e| {
        FlowyError::internal().with_context(format!("Failed to write to claude stdin: {}", e))
      })?;
    drop(stdin); // Close stdin to signal end of input
  }

  let stdout = child
    .stdout
    .take()
    .ok_or_else(|| FlowyError::internal().with_context("Failed to capture claude stdout"))?;

  // Parse the stream-json output
  let reader = BufReader::new(stdout);
  let lines = reader.lines();

  let stream = async_stream::stream! {
    let mut lines = lines;
    while let Ok(Some(line)) = lines.next_line().await {
      if line.trim().is_empty() {
        continue;
      }

      // Parse the JSON line from Claude CLI stream-json format
      // Format: {"type": "assistant", "subtype": "text", "text": "..."} (streaming delta)
      match serde_json::from_str::<serde_json::Value>(&line) {
        Ok(json) => {
          if let Some(text) = extract_text_from_stream_json(&json) {
            if !text.is_empty() {
              yield Ok(CompletionStreamValue::Answer { value: text });
            }
          }
        },
        Err(e) => {
          error!("Failed to parse claude stream JSON: {} - line: {}", e, line);
        },
      }
    }

    // Wait for the child process to finish
    match child.wait().await {
      Ok(status) => {
        if !status.success() {
          error!("Claude CLI exited with status: {}", status);
        }
      },
      Err(e) => {
        error!("Error waiting for claude CLI: {}", e);
      },
    }
  };

  Ok(stream.boxed())
}

/// Build the user message with full document context
fn build_user_message(params: &CompleteTextParams, completion_type: &CompletionType) -> String {
  let mut message = String::new();

  // Add completion history as context
  if let Some(ref meta) = params.metadata {
    if let Some(ref history) = meta.completion_history {
      if !history.is_empty() {
        // Separate document context from conversation history.
        // Document context messages are prefixed with "Full document context:"
        // (sent from the Dart side as system records, but role gets coerced to "ai" in protobuf).
        let mut has_doc_context = false;
        for msg in history {
          if msg.content.starts_with("Full document context:") {
            message.push_str(&msg.content);
            message.push_str("\n\n");
            has_doc_context = true;
          }
        }

        // Add conversation history (non-context messages)
        let conv_messages: Vec<_> = history
          .iter()
          .filter(|msg| !msg.content.starts_with("Full document context:"))
          .collect();
        if !conv_messages.is_empty() {
          message.push_str("Previous conversation:\n");
          for msg in conv_messages {
            message.push_str(&format!("[{}]: {}\n", msg.role, msg.content));
          }
          message.push('\n');
        }

        // If we have document context, clearly mark the selected text
        if has_doc_context {
          match completion_type {
            CompletionType::ImproveWriting => {
              message.push_str("The user has selected the following text to improve:\n---\n");
              message.push_str(&params.text);
              message.push_str("\n---\nPlease improve ONLY the selected text above. Output ONLY the improved text.");
              return message;
            },
            CompletionType::SpellingAndGrammar => {
              message.push_str(
                "The user has selected the following text to fix spelling and grammar:\n---\n",
              );
              message.push_str(&params.text);
              message.push_str(
                "\n---\nPlease fix ONLY the selected text above. Output ONLY the corrected text.",
              );
              return message;
            },
            CompletionType::MakeShorter => {
              message.push_str("The user has selected the following text to make shorter:\n---\n");
              message.push_str(&params.text);
              message.push_str(
                "\n---\nPlease shorten ONLY the selected text above. Output ONLY the shortened text.",
              );
              return message;
            },
            CompletionType::MakeLonger => {
              message.push_str("The user has selected the following text to expand:\n---\n");
              message.push_str(&params.text);
              message.push_str(
                "\n---\nPlease expand ONLY the selected text above. Output ONLY the expanded text.",
              );
              return message;
            },
            CompletionType::Explain => {
              message.push_str("The user has selected the following text to explain:\n---\n");
              message.push_str(&params.text);
              message.push_str("\n---\nPlease explain the selected text above.");
              return message;
            },
            _ => {
              // Fall through to default handling below
            },
          }
        }
      }
    }
  }

  // Default: Add the main text with appropriate framing (no document context available)
  match completion_type {
    CompletionType::ImproveWriting => {
      message.push_str("Please improve the following text:\n\n");
      message.push_str(&params.text);
    },
    CompletionType::SpellingAndGrammar => {
      message.push_str("Please fix spelling and grammar in the following text:\n\n");
      message.push_str(&params.text);
    },
    CompletionType::MakeShorter => {
      message.push_str("Please make the following text shorter:\n\n");
      message.push_str(&params.text);
    },
    CompletionType::MakeLonger => {
      message.push_str("Please expand the following text:\n\n");
      message.push_str(&params.text);
    },
    CompletionType::ContinueWriting => {
      message.push_str("Please continue writing from where this text ends:\n\n");
      message.push_str(&params.text);
    },
    CompletionType::Explain => {
      message.push_str("Please explain the following text:\n\n");
      message.push_str(&params.text);
    },
    CompletionType::AskAI | CompletionType::CustomPrompt => {
      message.push_str(&params.text);
    },
  }

  message
}

/// Extract text content from Claude CLI stream-json format
///
/// Claude CLI stream-json emits:
/// - `{"type":"assistant","subtype":"text","text":"..."}` for streaming text chunks
/// - `{"type":"result","result":"...","session_id":"..."}` for the final aggregated result
///
/// We only extract from "assistant" events (incremental deltas).
/// The "result" event contains the full concatenated text—emitting it would duplicate output.
fn extract_text_from_stream_json(json: &serde_json::Value) -> Option<String> {
  let event_type = json.get("type").and_then(|t| t.as_str())?;

  match event_type {
    // Streaming text chunks from assistant
    "assistant" => {
      if json.get("subtype").and_then(|s| s.as_str()) == Some("text") {
        return json
          .get("text")
          .and_then(|t| t.as_str())
          .map(|s| s.to_string());
      }
      None
    },
    // "result" contains the full aggregated text — skip to avoid duplication
    _ => None,
  }
}
