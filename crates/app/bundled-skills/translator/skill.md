---
name: translator
description: 翻译与本地化专家 — 支持多语种翻译、i18n 文件处理、术语管理、本地化测试。
metadata:
  display-name: 翻译本地化
  icon: "🌐"
  usage: "/translate <text> 或在对话中要求翻译"
---

# 翻译与本地化指令

协助用户完成翻译和本地化任务。

## 翻译服务

### 文本翻译
- 自动检测源语言 → 目标语言
- 专业术语保持一致性
- 代码/占位符保留不翻译
- 多行文本保持格式

### 技术文档翻译
- Markdown 格式保留
- 代码块不翻译
- 链接路径不翻译
- 专业术语首次出现保留原文（括号内翻译）

## i18n 文件处理

### 格式支持
- JSON（React Intl / Vue I18n）
- YAML（Ruby on Rails）
- PO/ POT（GNU Gettext）
- Properties（Java / Android）
- ARB（Flutter）
- XLIFF（通用）

### 操作方法

```bash
# 读取 i18n JSON 文件
python3 -c "
import json
with open('$FILE', 'r') as f:
    data = json.load(f)
def flat(d, prefix=''):
    for k, v in d.items():
        if isinstance(v, dict):
            flat(v, f'{prefix}{k}.')
        else:
            print(f'{prefix}{k} = {v}')
flat(data)
"
```

## 本地化检查清单

1. 文字截断（德语/俄语比英文长 30%+）
2. 日期格式（MM/DD vs DD/MM）
3. 数字格式（1,000.50 vs 1.000,50）
4. 货币符号位置（$10 vs 10€）
5. 时区处理
6. RTL 语言排版
7. 图标/符号的文化适配
8. 图片中的文字需要单独翻译

## 规则

- 代码中的字符串占位符 `{name}` / `%s` 必须保留
- HTML 标签必须保留
- 翻译后检查长度，确保 UI 不溢出
- 保持原文的语气和风格（正式/口语/技术）
