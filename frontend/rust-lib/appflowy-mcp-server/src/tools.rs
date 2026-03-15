use crate::document_xml::document_data_to_xml;
use crate::storage::AppFlowyStorage;
use serde_json::Value;
use tracing::info;

/// List all documents in the workspace
pub async fn list_documents(
  storage: &AppFlowyStorage,
  _arguments: &Value,
) -> anyhow::Result<String> {
  let views = storage.list_views()?;
  let documents: Vec<_> = views.iter().filter(|v| v.layout == "document").collect();

  let mut result = String::new();
  result.push_str(&format!("Found {} documents:\n\n", documents.len()));

  for doc in &documents {
    result.push_str(&format!(
      "- [{}] \"{}\" (last edited: {})\n",
      doc.id, doc.name, doc.last_edited
    ));
  }

  Ok(result)
}

/// Read a document in structured XML format
pub async fn read_document(storage: &AppFlowyStorage, arguments: &Value) -> anyhow::Result<String> {
  let doc_id = arguments
    .get("doc_id")
    .and_then(|v| v.as_str())
    .ok_or_else(|| anyhow::anyhow!("Missing required parameter: doc_id"))?;

  info!("Reading document: {}", doc_id);

  // Find the document title
  let views = storage.list_views()?;
  let title = views
    .iter()
    .find(|v| v.id == doc_id)
    .map(|v| v.name.clone())
    .unwrap_or_else(|| "Untitled".to_string());

  let data = storage.read_document_data(doc_id)?;
  let xml = document_data_to_xml(&data, &title, doc_id);

  Ok(xml)
}

/// Apply block operations to a document
pub async fn update_blocks(storage: &AppFlowyStorage, arguments: &Value) -> anyhow::Result<String> {
  let doc_id = arguments
    .get("doc_id")
    .and_then(|v| v.as_str())
    .ok_or_else(|| anyhow::anyhow!("Missing required parameter: doc_id"))?;

  let operations = arguments
    .get("operations")
    .and_then(|v| v.as_array())
    .ok_or_else(|| anyhow::anyhow!("Missing required parameter: operations"))?;

  info!(
    "Updating document {} with {} operations",
    doc_id,
    operations.len()
  );

  storage.update_document(doc_id, operations)?;

  Ok(format!(
    "Successfully applied {} operations to document {}",
    operations.len(),
    doc_id
  ))
}

/// Search documents by keyword
pub async fn search_documents(
  storage: &AppFlowyStorage,
  arguments: &Value,
) -> anyhow::Result<String> {
  let query = arguments
    .get("query")
    .and_then(|v| v.as_str())
    .ok_or_else(|| anyhow::anyhow!("Missing required parameter: query"))?;

  info!("Searching documents for: {}", query);

  let results = storage.search_documents(query)?;

  if results.is_empty() {
    return Ok(format!("No documents found matching \"{}\"", query));
  }

  let mut output = String::new();
  output.push_str(&format!(
    "Found {} results for \"{}\":\n\n",
    results.len(),
    query
  ));

  for result in &results {
    output.push_str(&format!(
      "- [{}] \"{}\"\n  Snippet: {}\n\n",
      result.doc_id, result.title, result.snippet
    ));
  }

  Ok(output)
}
