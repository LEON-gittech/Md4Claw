好，完整设计来了：

设计：AppFlowy × Claude Code 集成

两个模块

模块 1：appflowy-mcp-server（Rust 独立进程）
• Claude Code 通过 --mcp-config 连接
• 提供文档的完整读写能力
• 用于 Terminal 模式（外部操作文档）

模块 2：编辑器内 AI backend 替换
• 保留现有划词/斜杠菜单/工具栏 UX
• 把 cloud/Ollama 替换成 claude CLI 调用
• 用于 IDE 模式（编辑器内操作）

---

模块 1：MCP Server 设计

Tools：

| Tool | 输入 | 输出 |
|------|------|------|
| list_documents() | workspace_id | [{id, title, updated_at}] |
| read_document(doc_id) | doc_id | 结构化文档内容（见下方格式） |
| update_blocks(doc_id, operations) | doc_id + 操作列表 | success/error |
| search_documents(query) | 关键词 | 匹配的文档和段落 |

文档输出格式（给 model 看的）：

<document title="Meeting Notes" id="abc123">
  <heading level="1">Meeting Notes</heading>
  <paragraph>Today we discussed <bold>important</bold> topics.</paragraph>
  <todo checked="false">Follow up with team</todo>
  <todo checked="true">Send meeting recap</todo>
  <code language="python">
    def hello(): print("world")
  </code>
  <image src="/Users/.../images/uuid.png" width="500" height="300" />
  <callout icon="ℹ️">Remember to review before Friday</callout>
  <table>
    <row><cell>Name</cell><cell>Status</cell></row>
    <row><cell>Task A</cell><cell>Done</cell></row>
  </table>
  <toggle summary="Details">
    <paragraph>Hidden content here</paragraph>
  </toggle>
</document>


用类 XML 而不是 Markdown，因为：
• 保留完整的 block 类型信息（toggle、callout、todo 状态等）
• model 理解 XML 结构很好
• 可逆--能精确转回 Block JSON
• Markdown 会丢失 callout icon、todo 状态、toggle 嵌套等

update_blocks 操作格式：
[
  {"action": "insert", "after": "block_id", "block": {"type": "paragraph", "delta": [...]}},
  {"action": "update", "block_id": "xxx", "data": {"checked": true}},
  {"action": "delete", "block_id": "xxx"},
  {"action": "replace_text", "block_id": "xxx", "delta": [...]}
]


---

模块 2：编辑器内集成

替换点： ChatServiceMiddleware（flowy-ai/src/middleware/chat_service_mw.rs）

现在的路由：
is_local? → Ollama
else    → Cloud API


新增一个路由：
is_claude_code? → spawn `claude -p --print --output-format stream-json`
is_local?       → Ollama
else            → Cloud API


数据流（划词场景）：
1. 用户选中文本 → 现有逻辑把选区转成结构化文本（复用 NodeParser）
2. 构造 prompt（包含文档上下文 + 选中内容 + 操作类型）
3. spawn claude -p --print --output-format stream-json --system-prompt "..."
4. 流式读取 stdout → 解析 JSON events → 提取 content
5. 现有 MarkdownTextRobot 把结果写回编辑器

关键改进： 在编辑类操作（improve/fix grammar 等）中，除了选中文本，也传入完整文档上下文（你之前指出的 context 不足问题）。

---

依赖关系

appflowy-mcp-server (new Rust crate)
├── collab (existing - CRDT/Yjs)
├── collab-document (existing - document model)
├── flowy-document (existing - parsers, block types)
└── CollabKVDB (existing - RocksDB access)


编辑器集成改动：
flowy-ai/src/middleware/chat_service_mw.rs  (新增 Claude Code 路由)
flowy-ai/src/completion.rs                  (新增 Claude CLI spawner)
flowy-ai/src/local_ai/controller.rs         (Claude Code 配置)
setting_ai_view.dart                         (设置 UI 加 Claude Code 选项)


---

这个设计覆盖了你要的两种调用方式（Terminal + 编辑器内划词），结构化格式保留了完整的 block 信息。你觉得这个方向对吗？有什么要调整的？
