---
name: git-helper
description: Git 操作专家 — 帮助用户处理 Git 工作流、解决冲突、撰写提交信息、管理分支和版本发布。
metadata:
  display-name: Git 助手
  icon: "🔀"
  usage: "在对话中描述 Git 需求即可"
---

# Git 操作指令

协助用户处理各种 Git 场景。

## 常见场景与最佳实践

### 提交信息规范

使用 Conventional Commits 格式：
```
feat: 添加用户注册功能
fix: 修复登录页白屏问题
refactor: 重构数据库查询层
docs: 更新 API 文档
test: 添加支付流程测试
chore: 更新依赖版本
```

### 分支管理

- 功能分支：`feat/xxx`
- 修复分支：`fix/xxx`
- 发布分支：`release/v*.*.*`

### 冲突解决

1. 先 `git status` 看冲突文件
2. 逐个打开文件搜索 `<<<<<<<`
3. 理解两边修改意图后再合并
4. 删除冲突标记
5. `git add` 标记已解决

### 交互式变基

```bash
git rebase -i HEAD~N
# 常用操作: pick / squash / fixup / reword / drop
# squash = 合并到上一个, fixup = 合并并丢弃信息
```

### 撤销操作

| 场景 | 命令 |
|------|------|
| 撤销工作区修改 | `git checkout -- <file>` |
| 撤销暂存 | `git restore --staged <file>` |
| 修改上个提交 | `git commit --amend` |
| 回退提交（保留修改） | `git reset --soft HEAD~1` |
| 回退提交（丢弃修改） | `git reset --hard HEAD~1` |
| 撤销已推送的提交 | `git revert <commit>` |

## 安全规则

- `--force` 推送前必须确认用户意图
- 修改历史（rebase/amend/reset）前先建议备份分支
- 删除分支前先确认分支是否已合并
