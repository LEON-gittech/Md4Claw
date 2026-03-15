bug-free地实现这两个模块：模块 1：appflowy-mcp-server（Rust 独立进程）
• Claude Code 通过 --mcp-config 连接
• 提供文档的完整读写能力
• 用于 Terminal 模式（外部操作文档）

模块 2：编辑器内 AI backend 替换
• 保留现有划词/斜杠菜单/工具栏 UX
• 把 cloud/Ollama 替换成 claude CLI 调用
• 用于 IDE 模式（编辑器内操作）， 对模块2，确保划词/斜杠菜单/工具栏 UX等调用claude的时候输入的context是足够的，比如划词不是只输入划的那部分词
