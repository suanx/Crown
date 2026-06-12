# Changelog
## v1.3.5

### 🧹 清理
- 移除未使用的 `ProviderPanel.tsx` 死代码（已被 `ModelsPanel.tsx` 取代）
- 移除前端 `debug_test_stream` 死代码（后端未注册，UI 未调用）

## v1.3.4
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
