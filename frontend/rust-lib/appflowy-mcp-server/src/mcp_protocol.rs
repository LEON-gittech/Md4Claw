use crate::storage::AppFlowyStorage;
use crate::tools;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{error, info};

/// JSON-RPC request
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
  #[allow(dead_code)]
  pub jsonrpc: String,
  pub id: Option<Value>,
  pub method: String,
  #[serde(default)]
  pub params: Option<Value>,
}

/// JSON-RPC response
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
  pub jsonrpc: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub id: Option<Value>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub result: Option<Value>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
  pub code: i64,
  pub message: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub data: Option<Value>,
}

impl JsonRpcResponse {
  fn success(id: Option<Value>, result: Value) -> Self {
    Self {
      jsonrpc: "2.0".to_string(),
      id,
      result: Some(result),
      error: None,
    }
  }

  fn error(id: Option<Value>, code: i64, message: String) -> Self {
    Self {
      jsonrpc: "2.0".to_string(),
      id,
      result: None,
      error: Some(JsonRpcError {
        code,
        message,
        data: None,
      }),
    }
  }
}

pub struct McpServer {
  storage: AppFlowyStorage,
}

impl McpServer {
  pub fn new(storage: AppFlowyStorage) -> Self {
    Self { storage }
  }

  /// Run the MCP server over stdio (JSON-RPC over stdin/stdout)
  pub async fn run_stdio(self) -> anyhow::Result<()> {
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();

    while let Ok(Some(line)) = lines.next_line().await {
      let line = line.trim().to_string();
      if line.is_empty() {
        continue;
      }

      let response = match serde_json::from_str::<JsonRpcRequest>(&line) {
        Ok(request) => self.handle_request(request).await,
        Err(e) => Some(JsonRpcResponse::error(
          None,
          -32700,
          format!("Parse error: {}", e),
        )),
      };

      // Only send a response for requests (not notifications)
      if let Some(response) = response {
        let response_json = serde_json::to_string(&response)?;
        stdout.write_all(response_json.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
      }
    }

    Ok(())
  }

  async fn handle_request(&self, request: JsonRpcRequest) -> Option<JsonRpcResponse> {
    let id = request.id.clone();
    info!("MCP request: method={}", request.method);

    match request.method.as_str() {
      // MCP lifecycle
      "initialize" => Some(self.handle_initialize(id, request.params)),
      // JSON-RPC notification: MUST NOT send a response per MCP spec
      "notifications/initialized" => None,

      // MCP tool discovery
      "tools/list" => Some(self.handle_tools_list(id)),

      // MCP tool execution
      "tools/call" => Some(self.handle_tools_call(id, request.params).await),

      // Unknown method
      _ => Some(JsonRpcResponse::error(
        id,
        -32601,
        format!("Method not found: {}", request.method),
      )),
    }
  }

  fn handle_initialize(&self, id: Option<Value>, _params: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse::success(
      id,
      serde_json::json!({
        "protocolVersion": "2024-11-05",
        "capabilities": {
          "tools": {}
        },
        "serverInfo": {
          "name": "appflowy-mcp-server",
          "version": "0.1.0"
        }
      }),
    )
  }

  fn handle_tools_list(&self, id: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse::success(
      id,
      serde_json::json!({
        "tools": [
          {
            "name": "list_documents",
            "description": "List all documents in the AppFlowy workspace. Returns document IDs, titles, and last-edited timestamps.",
            "inputSchema": {
              "type": "object",
              "properties": {
                "workspace_id": {
                  "type": "string",
                  "description": "Optional workspace ID. If not provided, uses the default workspace."
                }
              },
              "required": []
            }
          },
          {
            "name": "read_document",
            "description": "Read the full content of an AppFlowy document in structured XML format. Preserves block types (headings, paragraphs, todo lists, callouts, code blocks, etc.) and rich text formatting (bold, italic, code, links).",
            "inputSchema": {
              "type": "object",
              "properties": {
                "doc_id": {
                  "type": "string",
                  "description": "The document ID to read"
                }
              },
              "required": ["doc_id"]
            }
          },
          {
            "name": "update_blocks",
            "description": "Apply block-level operations to an AppFlowy document. Supports insert, update, delete, and replace_text actions.",
            "inputSchema": {
              "type": "object",
              "properties": {
                "doc_id": {
                  "type": "string",
                  "description": "The document ID to update"
                },
                "operations": {
                  "type": "array",
                  "description": "List of operations to apply",
                  "items": {
                    "type": "object",
                    "properties": {
                      "action": {
                        "type": "string",
                        "enum": ["insert", "update", "delete", "replace_text"],
                        "description": "The operation type"
                      },
                      "block_id": {
                        "type": "string",
                        "description": "Target block ID (for update, delete, replace_text)"
                      },
                      "after": {
                        "type": "string",
                        "description": "Insert after this block ID (for insert)"
                      },
                      "block": {
                        "type": "object",
                        "description": "Block data for insert (type, delta, data fields)"
                      },
                      "data": {
                        "type": "object",
                        "description": "Data to update on existing block (for update)"
                      },
                      "delta": {
                        "type": "array",
                        "description": "Rich text delta array (for replace_text)"
                      }
                    },
                    "required": ["action"]
                  }
                }
              },
              "required": ["doc_id", "operations"]
            }
          },
          {
            "name": "search_documents",
            "description": "Search for documents by keyword. Returns matching documents with relevant text snippets.",
            "inputSchema": {
              "type": "object",
              "properties": {
                "query": {
                  "type": "string",
                  "description": "Search keyword or phrase"
                }
              },
              "required": ["query"]
            }
          }
        ]
      }),
    )
  }

  async fn handle_tools_call(&self, id: Option<Value>, params: Option<Value>) -> JsonRpcResponse {
    let params = match params {
      Some(p) => p,
      None => {
        return JsonRpcResponse::error(id, -32602, "Missing params".to_string());
      },
    };

    let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let arguments = params
      .get("arguments")
      .cloned()
      .unwrap_or(serde_json::json!({}));

    let result = match tool_name {
      "list_documents" => tools::list_documents(&self.storage, &arguments).await,
      "read_document" => tools::read_document(&self.storage, &arguments).await,
      "update_blocks" => tools::update_blocks(&self.storage, &arguments).await,
      "search_documents" => tools::search_documents(&self.storage, &arguments).await,
      _ => Err(anyhow::anyhow!("Unknown tool: {}", tool_name)),
    };

    match result {
      Ok(content) => JsonRpcResponse::success(
        id,
        serde_json::json!({
          "content": [
            {
              "type": "text",
              "text": content
            }
          ]
        }),
      ),
      Err(e) => {
        error!("Tool error: {}", e);
        JsonRpcResponse::success(
          id,
          serde_json::json!({
            "content": [
              {
                "type": "text",
                "text": format!("Error: {}", e)
              }
            ],
            "isError": true
          }),
        )
      },
    }
  }
}
