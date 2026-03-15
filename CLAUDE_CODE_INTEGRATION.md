# AppFlowy x Claude Code Integration Guide

This document describes two new modules that enable AppFlowy to use Claude Code as an AI backend, both for terminal-based document operations and for the in-editor AI writing assistant.

## Overview

| Module | Purpose | Mode |
|--------|---------|------|
| **appflowy-mcp-server** | Exposes AppFlowy documents to Claude Code via MCP | Terminal / CLI |
| **Editor AI Backend** | Replaces cloud AI routing with local Claude CLI | In-app editor |

Both modules require [Claude Code CLI](https://docs.anthropic.com/en/docs/claude-code) to be installed.

---

## Prerequisites

Install the Claude Code CLI:

```bash
npm install -g @anthropic-ai/claude-code
```

Verify it's available:

```bash
claude --version
```

---

## Module 1: AppFlowy MCP Server

The MCP server is a standalone Rust binary that lets Claude Code read, write, and search AppFlowy documents through the [Model Context Protocol](https://modelcontextprotocol.io/).

### Build

```bash
cd frontend/rust-lib
cargo build -p appflowy-mcp-server --release
```

The binary is produced at `target/release/appflowy-mcp-server`.

### Configure Claude Code

Create or edit `~/.claude/mcp.json`:

```json
{
  "mcpServers": {
    "appflowy": {
      "command": "/path/to/appflowy-mcp-server",
      "env": {
        "APPFLOWY_DATA_DIR": "/path/to/appflowy-data"
      }
    }
  }
}
```

**`APPFLOWY_DATA_DIR`**: Path to the AppFlowy user data directory. Typical locations:
- macOS: `~/Library/Application Support/AppFlowy/data`
- Linux: `~/.flowy/data`

If omitted, the server auto-detects the default location.

### Available MCP Tools

Once configured, Claude Code can use these tools:

| Tool | Description |
|------|-------------|
| `list_documents` | List all documents with IDs, titles, and timestamps |
| `read_document` | Read full document content in structured XML format |
| `update_blocks` | Insert, update, delete, or replace blocks in a document |
| `search_documents` | Search documents by keyword with text snippets |

### Usage Examples

After configuring `mcp.json`, start Claude Code and interact with your documents:

```
$ claude

> List all my AppFlowy documents
(Claude calls list_documents)

> Read the document titled "Meeting Notes"
(Claude calls read_document with the doc ID)

> Fix the spelling in the second paragraph
(Claude calls read_document, then update_blocks with replace_text)
```

---

## Module 2: Editor AI Backend (Claude Code CLI)

This module routes AppFlowy's in-editor AI writing commands through the local Claude Code CLI instead of cloud APIs.

### How It Works

1. When the Claude Code CLI is detected on the system, a **"Claude Code CLI (local)"** model appears in the AI model selector
2. Selecting this model routes all AI writing operations through the local `claude` CLI
3. The full document content is injected as context for text selection operations, ensuring Claude understands the surrounding content

### Selecting the Model

1. Open AppFlowy
2. Go to **Settings > AI Settings** (or the model selector in the AI toolbar)
3. Select **"Claude Code CLI (local)"** from the available models list

If the Claude CLI is not installed, this option will not appear.

### Supported Operations

All standard AI writing commands work with Claude Code:

| Command | Description |
|---------|-------------|
| **Ask AI** | Free-form questions about your document |
| **Improve Writing** | Rewrite selected text for clarity and flow |
| **Fix Spelling & Grammar** | Correct errors in selected text |
| **Make Shorter** | Condense selected text |
| **Make Longer** | Expand selected text with more detail |
| **Continue Writing** | Continue from where the text ends |
| **Explain** | Explain the selected text |

### Context Enrichment

When you select text and apply an AI operation, the system:

1. Captures the **full document content** as context
2. Sends it alongside the **selected text** to Claude
3. Claude sees both the overall document and the specific selection, producing more contextually relevant results

This is especially important for operations like "Improve Writing" or "Make Longer" where understanding the surrounding content leads to better output.

### Architecture

```
Flutter UI (selection/toolbar/slash menu)
    |
    v
AiWriterCubit (injects document context into records)
    |
    v
Protobuf Event (CompleteTextPB)
    |
    v
ChatServiceMiddleware (checks model name)
    |
    v  (model == "claude-code")
stream_complete_with_claude_code()
    |
    v
claude -p --output-format stream-json --system-prompt "..."
    |  (stdin: user message with context)
    v
Streaming response parsed and yielded back to UI
```

### Troubleshooting

**"Claude Code CLI is not installed" error**:
Install the CLI with `npm install -g @anthropic-ai/claude-code` and ensure `claude` is on your PATH.

**Model not appearing in selector**:
The system checks `which claude` at runtime. Make sure the CLI is installed and accessible from the shell environment where AppFlowy runs.

**Slow responses**:
The Claude CLI spawns a local process for each request. First-time invocations may be slower due to CLI initialization. Subsequent requests within the same session are typically faster.
