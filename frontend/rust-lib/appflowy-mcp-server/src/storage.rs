use collab::core::origin::CollabOrigin;
use collab::preclude::Collab;
use collab_document::document::Document;
use collab_folder::{Folder, ViewLayout};
use collab_plugins::CollabKVDB;
use collab_plugins::local_storage::kv::KVTransactionDB;
use collab_plugins::local_storage::kv::doc::CollabKVAction;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};

/// Represents a view (document) entry from the folder
#[derive(Debug, Clone, serde::Serialize)]
pub struct ViewEntry {
  pub id: String,
  pub name: String,
  pub layout: String,
  pub last_edited: i64,
  pub parent_id: String,
}

/// Storage layer that reads AppFlowy data from disk (RocksDB via CollabKVDB)
pub struct AppFlowyStorage {
  data_dir: PathBuf,
}

impl AppFlowyStorage {
  pub fn new(data_dir: Option<String>) -> anyhow::Result<Self> {
    let data_dir = match data_dir {
      Some(dir) => PathBuf::from(dir),
      None => Self::find_default_data_dir()?,
    };

    info!("AppFlowy data directory: {:?}", data_dir);

    if !data_dir.exists() {
      anyhow::bail!(
        "AppFlowy data directory does not exist: {:?}. Set APPFLOWY_DATA_DIR env var.",
        data_dir
      );
    }

    Ok(Self { data_dir })
  }

  fn find_default_data_dir() -> anyhow::Result<PathBuf> {
    let home = dirs_next().ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;

    let candidates = vec![
      home.join(".flowy"),
      home.join("AppFlowy"),
      home.join(".appflowy"),
      home.join("Library/Application Support/AppFlowy"),
    ];

    for candidate in &candidates {
      if candidate.exists() {
        return Ok(candidate.clone());
      }
    }

    anyhow::bail!(
      "Cannot find AppFlowy data directory. Tried: {:?}. Set APPFLOWY_DATA_DIR env var.",
      candidates
    )
  }

  /// Find the user data directory (contains workspace subdirs)
  fn find_user_data_dir(&self) -> anyhow::Result<PathBuf> {
    let mut user_dirs = Vec::new();
    for entry in std::fs::read_dir(&self.data_dir)? {
      let entry = entry?;
      let path = entry.path();
      if path.is_dir() {
        if path.join("collab_db").exists() || path.join("0_collab_db").exists() {
          user_dirs.push(path);
        }
      }
    }

    if user_dirs.is_empty() {
      if self.data_dir.join("collab_db").exists() || self.data_dir.join("0_collab_db").exists() {
        return Ok(self.data_dir.clone());
      }
      anyhow::bail!("No user data directory found in {:?}", self.data_dir);
    }

    // Use the most recently modified user directory
    user_dirs.sort_by(|a, b| {
      let a_time = std::fs::metadata(a)
        .and_then(|m| m.modified())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
      let b_time = std::fs::metadata(b)
        .and_then(|m| m.modified())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
      b_time.cmp(&a_time)
    });

    Ok(user_dirs[0].clone())
  }

  /// Open the CollabKVDB for a user
  fn open_collab_db(&self) -> anyhow::Result<Arc<CollabKVDB>> {
    let user_dir = self.find_user_data_dir()?;

    let db_paths = vec![user_dir.join("collab_db"), user_dir.join("0_collab_db")];

    for db_path in &db_paths {
      if db_path.exists() {
        info!("Opening collab DB at: {:?}", db_path);
        let db = CollabKVDB::open(db_path)
          .map_err(|e| anyhow::anyhow!("Failed to open collab DB: {}", e))?;
        return Ok(Arc::new(db));
      }
    }

    anyhow::bail!("No collab DB found in {:?}", user_dir)
  }

  /// Find the workspace ID
  fn find_workspace_id(&self) -> anyhow::Result<String> {
    let user_dir = self.find_user_data_dir()?;

    // Try to read workspace_id from session_cache
    let workspace_file = user_dir.join("session_cache");
    if workspace_file.exists() {
      if let Ok(content) = std::fs::read_to_string(&workspace_file) {
        if let Ok(val) = serde_json::from_str::<Value>(&content) {
          if let Some(wid) = val.get("workspace_id").and_then(|v| v.as_str()) {
            return Ok(wid.to_string());
          }
        }
      }
    }

    // Try reading UUID-named directories
    for entry in std::fs::read_dir(&user_dir)? {
      let entry = entry?;
      let name = entry.file_name().to_string_lossy().to_string();
      if uuid::Uuid::parse_str(&name).is_ok() && entry.path().is_dir() {
        return Ok(name);
      }
    }

    anyhow::bail!("Cannot determine workspace ID")
  }

  /// Load folder data to list views
  pub fn list_views(&self) -> anyhow::Result<Vec<ViewEntry>> {
    let db = self.open_collab_db()?;
    let workspace_id = self.find_workspace_id()?;
    let uid = self.find_uid()?;

    let collab = self.load_collab(&db, uid, &workspace_id, &workspace_id)?;
    let folder = Folder::open(uid, collab, None)
      .map_err(|e| anyhow::anyhow!("Failed to open folder: {}", e))?;

    let mut views = Vec::new();
    let folder_data = folder.get_folder_data(&workspace_id);
    if let Some(data) = folder_data {
      for view in &data.views {
        let layout_str = match view.layout {
          ViewLayout::Document => "document",
          ViewLayout::Grid => "grid",
          ViewLayout::Board => "board",
          ViewLayout::Calendar => "calendar",
          ViewLayout::Chat => "chat",
        };
        views.push(ViewEntry {
          id: view.id.clone(),
          name: view.name.clone(),
          layout: layout_str.to_string(),
          last_edited: view.last_edited_time,
          parent_id: view.parent_view_id.clone(),
        });
      }
    }

    Ok(views)
  }

  /// Read a document's full data
  pub fn read_document_data(
    &self,
    doc_id: &str,
  ) -> anyhow::Result<collab_document::blocks::DocumentData> {
    let db = self.open_collab_db()?;
    let workspace_id = self.find_workspace_id()?;
    let uid = self.find_uid()?;

    let collab = self.load_collab(&db, uid, &workspace_id, doc_id)?;
    let document = Document::open(collab)
      .map_err(|e| anyhow::anyhow!("Failed to open document {}: {}", doc_id, e))?;

    let data = document
      .get_document_data()
      .map_err(|e| anyhow::anyhow!("Failed to get document data: {}", e))?;

    Ok(data)
  }

  /// Apply operations to a document
  pub fn update_document(&self, doc_id: &str, operations: &[Value]) -> anyhow::Result<()> {
    let db = self.open_collab_db()?;
    let workspace_id = self.find_workspace_id()?;
    let uid = self.find_uid()?;

    let collab = self.load_collab(&db, uid, &workspace_id, doc_id)?;
    let mut document = Document::open(collab)
      .map_err(|e| anyhow::anyhow!("Failed to open document {}: {}", doc_id, e))?;

    for op in operations {
      let action = op
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'action' in operation"))?;

      match action {
        "insert" => {
          self.apply_insert(&mut document, op)?;
        },
        "update" => {
          self.apply_update(&mut document, op)?;
        },
        "delete" => {
          self.apply_delete(&mut document, op)?;
        },
        "replace_text" => {
          self.apply_replace_text(&mut document, op)?;
        },
        _ => {
          warn!("Unknown operation action: {}", action);
        },
      }
    }

    // Flush changes back to the DB
    let encoded = document
      .encode_collab()
      .map_err(|e| anyhow::anyhow!("Failed to encode collab: {}", e))?;

    let write_txn = db.write_txn();
    write_txn
      .flush_doc(
        uid,
        &workspace_id,
        doc_id,
        encoded.state_vector.to_vec(),
        encoded.doc_state.to_vec(),
      )
      .map_err(|e| anyhow::anyhow!("Failed to flush document: {}", e))?;
    write_txn
      .commit_transaction()
      .map_err(|e| anyhow::anyhow!("Failed to commit transaction: {}", e))?;

    Ok(())
  }

  /// Search documents by keyword
  pub fn search_documents(&self, query: &str) -> anyhow::Result<Vec<SearchResult>> {
    let views = self.list_views()?;
    let query_lower = query.to_lowercase();
    let mut results = Vec::new();

    // Search by title first
    for view in &views {
      if view.layout != "document" {
        continue;
      }
      if view.name.to_lowercase().contains(&query_lower) {
        results.push(SearchResult {
          doc_id: view.id.clone(),
          title: view.name.clone(),
          snippet: format!("Title match: {}", view.name),
          score: 1.0,
        });
      }
    }

    // Search document content
    for view in &views {
      if view.layout != "document" {
        continue;
      }
      if results.iter().any(|r| r.doc_id == view.id) {
        continue;
      }

      match self.read_document_data(&view.id) {
        Ok(data) => {
          let text = self.extract_plain_text(&data);
          if let Some(snippet) = find_snippet(&text, &query_lower) {
            results.push(SearchResult {
              doc_id: view.id.clone(),
              title: view.name.clone(),
              snippet,
              score: 0.5,
            });
          }
        },
        Err(e) => {
          warn!("Failed to read document {} for search: {}", view.id, e);
        },
      }
    }

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    Ok(results)
  }

  // --- Internal helpers ---

  fn find_uid(&self) -> anyhow::Result<i64> {
    let user_dir = self.find_user_data_dir()?;
    if let Some(name) = user_dir.file_name() {
      if let Ok(uid) = name.to_string_lossy().parse::<i64>() {
        return Ok(uid);
      }
    }
    // Default to 0 if we can't determine uid
    Ok(0)
  }

  fn load_collab(
    &self,
    db: &Arc<CollabKVDB>,
    uid: i64,
    workspace_id: &str,
    object_id: &str,
  ) -> anyhow::Result<Collab> {
    // Create a new Collab and load state from DB using the persistence API
    let mut collab = Collab::new_with_origin(CollabOrigin::Empty, object_id, vec![], false);
    let read_txn = db.read_txn();
    if read_txn.is_exist(uid, workspace_id, object_id) {
      let mut txn = collab.transact_mut();
      read_txn
        .load_doc_with_txn(uid, workspace_id, object_id, &mut txn)
        .map_err(|e| anyhow::anyhow!("Failed to load collab {}: {}", object_id, e))?;
      drop(txn);
    } else {
      anyhow::bail!("No collab data found for object: {}", object_id);
    }

    Ok(collab)
  }

  fn apply_insert(&self, document: &mut Document, op: &Value) -> anyhow::Result<()> {
    let after_id = op.get("after").and_then(|v| v.as_str()).unwrap_or("");
    let block = op
      .get("block")
      .ok_or_else(|| anyhow::anyhow!("Missing 'block' in insert operation"))?;
    let block_type = block
      .get("type")
      .and_then(|v| v.as_str())
      .unwrap_or("paragraph");

    // Get parent from the 'after' block's parent
    let doc_data = document
      .get_document_data()
      .map_err(|e| anyhow::anyhow!("Failed to get document data: {}", e))?;

    let parent_id = if after_id.is_empty() {
      doc_data.page_id.clone()
    } else {
      doc_data
        .blocks
        .get(after_id)
        .map(|b| b.parent.clone())
        .unwrap_or(doc_data.page_id.clone())
    };

    let mut block_data = HashMap::new();
    if let Some(data) = block.get("data").and_then(|v| v.as_object()) {
      for (k, v) in data {
        block_data.insert(k.clone(), v.clone());
      }
    }

    let block_id = gen_block_id();
    let text_id = gen_block_id();

    // Build delta text if provided
    let delta_json = block
      .get("delta")
      .map(|d| serde_json::to_string(d).unwrap_or_default())
      .unwrap_or_else(|| "[]".to_string());

    // apply_text_delta takes owned String and returns ()
    #[allow(deprecated)]
    document.create_text(&text_id, delta_json.clone());

    document
      .insert_block(
        collab_document::blocks::Block {
          id: block_id.clone(),
          ty: block_type.to_string(),
          parent: parent_id,
          children: gen_block_id(),
          data: block_data,
          external_id: Some(text_id),
          external_type: Some("text".to_string()),
        },
        Some(after_id.to_string()),
      )
      .map_err(|e| anyhow::anyhow!("Failed to insert block: {}", e))?;

    Ok(())
  }

  fn apply_update(&self, document: &mut Document, op: &Value) -> anyhow::Result<()> {
    let block_id = op
      .get("block_id")
      .and_then(|v| v.as_str())
      .ok_or_else(|| anyhow::anyhow!("Missing 'block_id' in update operation"))?;
    let data = op
      .get("data")
      .ok_or_else(|| anyhow::anyhow!("Missing 'data' in update operation"))?;

    let mut update_data = HashMap::new();
    if let Some(obj) = data.as_object() {
      for (k, v) in obj {
        update_data.insert(k.clone(), v.clone());
      }
    }

    document
      .update_block(block_id, update_data)
      .map_err(|e| anyhow::anyhow!("Failed to update block {}: {}", block_id, e))?;

    Ok(())
  }

  fn apply_delete(&self, document: &mut Document, op: &Value) -> anyhow::Result<()> {
    let block_id = op
      .get("block_id")
      .and_then(|v| v.as_str())
      .ok_or_else(|| anyhow::anyhow!("Missing 'block_id' in delete operation"))?;

    document
      .delete_block(block_id)
      .map_err(|e| anyhow::anyhow!("Failed to delete block {}: {}", block_id, e))?;

    Ok(())
  }

  fn apply_replace_text(&self, document: &mut Document, op: &Value) -> anyhow::Result<()> {
    let block_id = op
      .get("block_id")
      .and_then(|v| v.as_str())
      .ok_or_else(|| anyhow::anyhow!("Missing 'block_id' in replace_text operation"))?;
    let delta = op
      .get("delta")
      .ok_or_else(|| anyhow::anyhow!("Missing 'delta' in replace_text operation"))?;

    let delta_json = serde_json::to_string(delta)?;

    // Get the text_id from the block
    let doc_data = document
      .get_document_data()
      .map_err(|e| anyhow::anyhow!("Failed to get document data: {}", e))?;

    let text_id = doc_data
      .blocks
      .get(block_id)
      .and_then(|b| b.external_id.clone())
      .ok_or_else(|| anyhow::anyhow!("Block {} has no text", block_id))?;

    // apply_text_delta takes owned String and returns ()
    document.apply_text_delta(&text_id, delta_json);

    Ok(())
  }

  fn extract_plain_text(&self, data: &collab_document::blocks::DocumentData) -> String {
    let mut text = String::new();
    let root_id = &data.page_id;
    self.collect_text_recursive(data, root_id, &mut text);
    text
  }

  fn collect_text_recursive(
    &self,
    data: &collab_document::blocks::DocumentData,
    block_id: &str,
    text: &mut String,
  ) {
    if let Some(block) = data.blocks.get(block_id) {
      if let Some(text_map) = &data.meta.text_map {
        if let Some(ext_id) = &block.external_id {
          if let Some(delta_str) = text_map.get(ext_id) {
            if let Ok(deltas) = serde_json::from_str::<Vec<Value>>(delta_str) {
              for delta in &deltas {
                if let Some(insert) = delta.get("insert").and_then(|v| v.as_str()) {
                  text.push_str(insert);
                }
              }
              text.push('\n');
            }
          }
        }
      }

      if let Some(children_ids) = data.meta.children_map.get(&block.children) {
        for child_id in children_ids {
          self.collect_text_recursive(data, child_id, text);
        }
      }
    }
  }
}

fn dirs_next() -> Option<PathBuf> {
  std::env::var("HOME")
    .ok()
    .map(PathBuf::from)
    .or_else(|| std::env::var("USERPROFILE").ok().map(PathBuf::from))
}

fn gen_block_id() -> String {
  uuid::Uuid::new_v4().to_string()
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchResult {
  pub doc_id: String,
  pub title: String,
  pub snippet: String,
  pub score: f64,
}

/// Find a snippet around the query in the text
fn find_snippet(text: &str, query: &str) -> Option<String> {
  let lower = text.to_lowercase();
  let pos = lower.find(query)?;

  // Calculate byte offsets then adjust to char boundaries to avoid UTF-8 panics
  let raw_start = pos.saturating_sub(80);
  let raw_end = std::cmp::min(pos + query.len() + 80, text.len());

  // Find nearest char boundary at or after raw_start
  let start = text
    .char_indices()
    .map(|(i, _)| i)
    .find(|&i| i >= raw_start)
    .unwrap_or(0);
  // Find nearest char boundary at or before raw_end
  let end = text
    .char_indices()
    .map(|(i, c)| i + c.len_utf8())
    .filter(|&i| i <= raw_end)
    .last()
    .unwrap_or(text.len());

  let snippet = &text[start..end];
  let mut result = String::new();
  if start > 0 {
    result.push_str("...");
  }
  result.push_str(snippet.trim());
  if end < text.len() {
    result.push_str("...");
  }
  Some(result)
}
