use collab_document::blocks::DocumentData;
use serde_json::Value;

/// Convert AppFlowy DocumentData to structured XML format for LLM consumption.
///
/// Uses XML rather than Markdown to preserve block types (toggle, callout, todo state, etc.)
/// that would be lost in Markdown conversion. The format is designed to be:
/// - Reversible back to Block JSON
/// - Easy for LLMs to understand and generate
/// - Preserving all structural information
pub fn document_data_to_xml(data: &DocumentData, title: &str, doc_id: &str) -> String {
  let mut xml = String::new();
  xml.push_str(&format!(
    "<document title=\"{}\" id=\"{}\">\n",
    escape_xml(title),
    escape_xml(doc_id)
  ));

  let root_id = &data.page_id;
  if let Some(children_ids) = data
    .blocks
    .get(root_id)
    .and_then(|b| data.meta.children_map.get(&b.children))
  {
    for child_id in children_ids {
      block_to_xml(data, child_id, &mut xml, 1);
    }
  }

  xml.push_str("</document>\n");
  xml
}

fn block_to_xml(data: &DocumentData, block_id: &str, xml: &mut String, indent: usize) {
  let block = match data.blocks.get(block_id) {
    Some(b) => b,
    None => return,
  };

  let indent_str = "  ".repeat(indent);
  let delta_text = get_rich_text(data, block_id);

  match block.ty.as_str() {
    "heading" => {
      let level = block
        .data
        .get("level")
        .and_then(|v| v.as_u64())
        .unwrap_or(1);
      xml.push_str(&format!(
        "{}<heading level=\"{}\" id=\"{}\">",
        indent_str, level, block.id
      ));
      xml.push_str(&delta_text);
      xml.push_str("</heading>\n");
    },

    "paragraph" => {
      xml.push_str(&format!("{}<paragraph id=\"{}\">", indent_str, block.id));
      xml.push_str(&delta_text);
      xml.push_str("</paragraph>\n");
    },

    "todo_list" => {
      let checked = block
        .data
        .get("checked")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
      xml.push_str(&format!(
        "{}<todo checked=\"{}\" id=\"{}\">",
        indent_str, checked, block.id
      ));
      xml.push_str(&delta_text);
      xml.push_str("</todo>\n");
    },

    "bulleted_list" => {
      xml.push_str(&format!(
        "{}<bulleted_list id=\"{}\">",
        indent_str, block.id
      ));
      xml.push_str(&delta_text);
      append_children(data, block, xml, indent);
      xml.push_str(&format!("{}</bulleted_list>\n", indent_str));
    },

    "numbered_list" => {
      let number = block
        .data
        .get("number")
        .and_then(|v| v.as_u64())
        .unwrap_or(1);
      xml.push_str(&format!(
        "{}<numbered_list number=\"{}\" id=\"{}\">",
        indent_str, number, block.id
      ));
      xml.push_str(&delta_text);
      append_children(data, block, xml, indent);
      xml.push_str(&format!("{}</numbered_list>\n", indent_str));
    },

    "toggle_list" => {
      let collapsed = block
        .data
        .get("collapsed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
      xml.push_str(&format!(
        "{}<toggle collapsed=\"{}\" id=\"{}\">",
        indent_str, collapsed, block.id
      ));
      xml.push_str(&delta_text);
      xml.push('\n');
      append_children(data, block, xml, indent);
      xml.push_str(&format!("{}</toggle>\n", indent_str));
    },

    "quote" => {
      xml.push_str(&format!("{}<quote id=\"{}\">", indent_str, block.id));
      xml.push_str(&delta_text);
      append_children(data, block, xml, indent);
      xml.push_str(&format!("{}</quote>\n", indent_str));
    },

    "callout" => {
      let icon = block
        .data
        .get("icon")
        .and_then(|v| v.as_str())
        .unwrap_or("");
      xml.push_str(&format!(
        "{}<callout icon=\"{}\" id=\"{}\">",
        indent_str,
        escape_xml(icon),
        block.id
      ));
      xml.push_str(&delta_text);
      append_children(data, block, xml, indent);
      xml.push_str(&format!("{}</callout>\n", indent_str));
    },

    "code" => {
      let language = block
        .data
        .get("language")
        .and_then(|v| v.as_str())
        .unwrap_or("");
      xml.push_str(&format!(
        "{}<code language=\"{}\" id=\"{}\">\n",
        indent_str,
        escape_xml(language),
        block.id
      ));
      xml.push_str(&format!("{}{}\n", indent_str, delta_text));
      xml.push_str(&format!("{}</code>\n", indent_str));
    },

    "math_equation" => {
      let formula = block
        .data
        .get("formula")
        .and_then(|v| v.as_str())
        .unwrap_or("");
      xml.push_str(&format!(
        "{}<math_equation id=\"{}\">{}</math_equation>\n",
        indent_str,
        block.id,
        escape_xml(formula)
      ));
    },

    "image" => {
      let url = block.data.get("url").and_then(|v| v.as_str()).unwrap_or("");
      let width = block
        .data
        .get("width")
        .and_then(|v| v.as_f64())
        .map(|v| format!(" width=\"{}\"", v as i64))
        .unwrap_or_default();
      let height = block
        .data
        .get("height")
        .and_then(|v| v.as_f64())
        .map(|v| format!(" height=\"{}\"", v as i64))
        .unwrap_or_default();
      let caption = block
        .data
        .get("caption")
        .and_then(|v| v.as_str())
        .unwrap_or("");
      xml.push_str(&format!(
        "{}<image src=\"{}\" id=\"{}\"{}{} caption=\"{}\" />\n",
        indent_str,
        escape_xml(url),
        block.id,
        width,
        height,
        escape_xml(caption)
      ));
    },

    "divider" => {
      xml.push_str(&format!("{}<divider id=\"{}\" />\n", indent_str, block.id));
    },

    // Fallback for unknown block types
    other => {
      xml.push_str(&format!(
        "{}<block type=\"{}\" id=\"{}\">",
        indent_str,
        escape_xml(other),
        block.id
      ));
      xml.push_str(&delta_text);
      append_children(data, block, xml, indent);
      xml.push_str(&format!("{}</block>\n", indent_str));
    },
  }
}

fn append_children(
  data: &DocumentData,
  block: &collab_document::blocks::Block,
  xml: &mut String,
  indent: usize,
) {
  if let Some(children_ids) = data.meta.children_map.get(&block.children) {
    if !children_ids.is_empty() {
      for child_id in children_ids {
        block_to_xml(data, child_id, xml, indent + 1);
      }
    }
  }
}

/// Extract rich text from a block, converting delta to inline XML
fn get_rich_text(data: &DocumentData, block_id: &str) -> String {
  let text_map = match &data.meta.text_map {
    Some(tm) => tm,
    None => return String::new(),
  };

  let block = match data.blocks.get(block_id) {
    Some(b) => b,
    None => return String::new(),
  };

  let text_id = match &block.external_id {
    Some(id) => id,
    None => return String::new(),
  };

  let delta_str = match text_map.get(text_id) {
    Some(s) => s,
    None => return String::new(),
  };

  let deltas: Vec<Value> = match serde_json::from_str(delta_str) {
    Ok(d) => d,
    Err(_) => return String::new(),
  };

  let mut result = String::new();
  for delta in &deltas {
    let insert = delta.get("insert").and_then(|v| v.as_str()).unwrap_or("");
    let attrs = delta
      .get("attributes")
      .and_then(|v| v.as_object())
      .cloned()
      .unwrap_or_default();

    if attrs.is_empty() {
      result.push_str(&escape_xml(insert));
    } else {
      let mut text = escape_xml(insert);

      // Wrap with formatting tags (innermost first)
      if attrs.contains_key("code") {
        text = format!("<code>{}</code>", text);
      }
      if attrs.contains_key("bold") {
        text = format!("<bold>{}</bold>", text);
      }
      if attrs.contains_key("italic") {
        text = format!("<italic>{}</italic>", text);
      }
      if attrs.contains_key("underline") {
        text = format!("<underline>{}</underline>", text);
      }
      if attrs.contains_key("strikethrough") {
        text = format!("<strikethrough>{}</strikethrough>", text);
      }
      if let Some(href) = attrs.get("href").and_then(|v| v.as_str()) {
        text = format!("<link href=\"{}\">{}</link>", escape_xml(href), text);
      }

      result.push_str(&text);
    }
  }

  result
}

fn escape_xml(s: &str) -> String {
  s.replace('&', "&amp;")
    .replace('<', "&lt;")
    .replace('>', "&gt;")
    .replace('"', "&quot;")
    .replace('\'', "&apos;")
}
