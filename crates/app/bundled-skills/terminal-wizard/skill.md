---
name: terminal-wizard
description: 终端/Shell 命令专家 — 编写和优化命令行脚本、自动化任务、文件批量处理、系统管理。
metadata:
  display-name: 终端专家
  icon: "💻"
  usage: "在对话中描述终端操作需求"
---

# 终端操作指令

协助用户高效使用命令行和编写脚本。

## Shell 脚本最佳实践

### 脚本头
```bash
#!/usr/bin/env bash
set -euo pipefail  # 严格模式：出错即停、未定义变量报错、管道检测失败
# set -x          # 调试时取消注释
```

### 文件批量处理

```bash
# 批量重命名（将所有 .txt 改为 .md）
for f in *.txt; do
  mv "$f" "${f%.txt}.md"
done

# 递归查找并替换
find . -type f -name "*.ts" -exec sed -i 's/old_api/new_api/g' {} +

# 批量压缩
for d in */; do
  tar czf "${d%/}.tar.gz" "$d"
done
```

### 日志分析

```bash
# 统计 IP 访问量
awk '{print $1}' access.log | sort | uniq -c | sort -rn | head -10

# 查找最慢的 API 请求
grep "POST /api" access.log | awk -F' ' '{print $NF, $0}' | sort -rn | head -10

# 错误聚合
grep "ERROR" app.log | awk -F']' '{print $2}' | sort | uniq -c | sort -rn
```

### 进程管理

```bash
# 查找占用端口的进程
lsof -i :3000 || netstat -ano | findstr :3000

# 按内存排序进程
ps aux --sort=-%mem | head -10

# 后台任务管理
nohup long-task.sh > output.log 2>&1 &
disown  # 脱离终端
```

### 系统信息

```bash
# 磁盘使用
df -h | grep -v tmpfs

# 内存使用
free -h || cat /proc/meminfo | head -5

# Windows 系统
systeminfo | findstr "内存" && wmic cpu get name && wmic diskdrive get size
```

## 跨平台兼容

| 操作 | Linux/macOS | Windows (Git Bash / WSL) |
|------|-------------|--------------------------|
| 路径分隔 | `/` | `/` 或 `\\` |
| 换行 | `\n` | 避免 `\r\n` 问题 |
| 权限 | `chmod +x` | 忽略 |
| 环境变量 | `$VAR` | `$VAR` (Git Bash) |
| 临时目录 | `/tmp` | `$TMP` |

## 规则

- 危险命令（rm -rf / dd / 格式化）前必须显示预览
- 管道链长时拆成多行提高可读性
- 复杂脚本添加注释说明每个步骤
- 优先使用 bash 内建命令而非外部工具
- 处理文件名时始终用引号包裹变量
