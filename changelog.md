# Changelog

## v1.3.11

### 🐛 修复
- 修复讯飞星辰 MaaS 发消息无回复：`stream_options` 仅对 DeepSeek 发送，其他供应商省略
- 修复除 deepseek-v4-flash 外其他模型不显示思考文字：非 DeepSeek 供应商也传递 `reasoning_effort`
- SSE 解析新增 JSON fallback：`parse_sse_body()` 在零 chunk 时自动解析 JSON 响应

### 🔧 变更
- `ChatOpts` 新增 `stream_options` 字段，按供应商按需传递
- `stream_inner()` 改为读取完整响应字节后解析，支持 SSE 和 JSON 两种格式
- 新增 `parse_sse_body()` 和 `try_parse_as_json_fallback()` 函数

## v1.3.10
- 修复 `stream_with_opts()` 缺失 `thinking` 字段导致编译错误
- 移除未使用的 `futures::stream::self` 导入警告

## v1.3.9
### 🐛 修复
- 修复 iFlytek（讯飞）等接口发消息无回复的问题
  - `reasoning_effort` 不再发送给不支持该参数的供应商
  - 流式请求支持非 SSE 的 JSON 回落（`try_parse_as_single_chunk`）
  - 修复 `stream_options` 兼容性导致的空响应

### 🔧 变更
- `turn_chat_opts()` 中 `ProviderId::Other` 不再发送 `reasoning_effort`
- `stream_inner()` 新增 Content-Type 检测：`application/json` 走 JSON 回落解析
- 新增 `try_parse_as_single_chunk()` 函数

## v1.3.8
### ✨ 新功能
- 自定义上下文长度：设置 → 模型供应商 中可配置每个模型的上下文窗口大小，引擎实时生效
- 自定义工作目录：新建线程时自动使用设置中的工作目录作为默认路径
- Anthropic / OpenAI 定价表：为 Claude Sonnet/Haiku/Opus、GPT-5.5/o4 等模型提供费用估算

### 🐛 修复
- 修复自定义工作目录保存后不生效的问题（new thread + send_message 路径均已覆盖）
- 修复设置中的上下文长度配置被 `pricing::context_window()` 忽略的问题
- 修复 `ProviderId` 枚举缺少 Anthropic/OpenAI 变体导致 match 不完整

### 🔧 变更
- `ThreadState` 新增 `context_window_override` 字段，支持线程级上下文窗口自定义
- `AgentEngine` 新增 `context_window_overrides` 映射表 + `set_context_window_overrides()` 方法
- `pricing::context_window()` 新增 `custom_override` 参数
- 前端 `WelcomePage` 创建线程时自动传入工作目录
- 后端 `send_message` / `create_thread` 命令使用工作目录作为默认 cwd

## v1.3.7
## v1.3.6
### 🐛 修复
- 修复 `set_config` 中 `json.remove("workspaceDir")` Rust 编译错误（`serde_json::Value` 无 `.remove()` 方法）
- 修复 `template.rs` 中 irrefutable `if let` 编译器警告
- 修复 `fs.rs` 中 `mut` 未被使用编译器警告

## v1.3.5

### 🧹 清理
- 移除未使用的 `ProviderPanel.tsx` 死代码（已被 `ModelsPanel.tsx` 取代）
- 移除前端 `debug_test_stream` 死代码（后端未注册，UI 未调用）

## v1.3.4
### 📝 文档
- README 全面重写：新增架构图、数据流说明、技术栈表格、完整项目结构树
- 新增内置技能对比表、Web Search 供应商表、隐私安全说明

### 🐛 修复
- 修复 CI 中 `workspacepanel.tsx` 大小写不匹配导致的 TS1261 编译错误
- 修复设置中"工作目录"保存后未持久化的问题



## v1.3.3

### 🧠 AI 增强
- 强化 system prompt 思考指引，鼓励模型深入分析
- 默认推理强度从 `medium` 改为 `high`，回答更准确
- 思考过程实时展开显示，不再自动折叠

### 📚 11 个内置技能
应用首次启动自动安装，开机即用：

| 技能 | 用途 |
|------|------|
| deep-research | 系统性多维度调研报告 |
| web-research | 联网搜索与事实核查 |
| file-reader | PDF/Word/Excel/PPT 文档读取 |
| code-review | 代码审查（逻辑/安全/性能） |
| git-helper | Git 工作流与冲突解决 |
| db-helper | SQL/数据库设计与优化 |
| image-analyzer | OCR/图片处理/截图对比 |
| api-tester | API 测试与调试 |
| refactoring | 代码重构安全指南 |
| translator | 翻译与 i18n 本地化 |
| terminal-wizard | Shell 脚本与终端技巧 |

### 🎨 界面
- **5 套视觉主题**：经典 / 简约 / 明快 / 暖纸 / OLED
- **7 种配色**：深蓝 / 海洋 / 紫罗兰 / 火焰 / 玫瑰 / 森林 / 午夜
- 回复头像改为品牌蓝 🧠 人脑图标
- 加载 Spinner 改为脉冲人脑动画
- 左侧栏简化：移除「本地」「ME 头像」和余额信息
- 输入框 Tab 缩进 + 字数统计

### ⚙️ 设置
- **子任务配置**：设置→能力，最大并发数（1-20）+ 子任务模型选择
- **技能管理增强**：删除按钮、来源标签、路径显示、使用指引
- **长期记忆导出**：设置→关于→数据→下载 AGENTS.md

### 📷 图片理解
- 前端支持图片粘贴/拖拽上传
- 后端自动转为 OpenAI 视觉协议格式
- 支持 MiMo/OpenAI 兼容等多模态模型

### 🔧 修复
- SSE 流式兼容讯飞（`data:` 无空格前缀）
- 供应商测试命令 `debug_test_provider` 修复
- 清除残余旧大小写文件名
- 多模态消息格式修复
- 讯飞预设模型删除

### 🏷️ 版本
- 版本号 v1.3.2
- 新桌面图标
- README 全面更新
- GitHub Actions 自动构建 Windows 安装包
