---
name: db-helper
description: 数据库查询与分析专家 — 编写 SQL、优化查询、设计表结构、数据迁移、排查性能问题。
metadata:
  display-name: 数据库助手
  icon: "🗄️"
  usage: "在对话中描述数据库需求即可"
---

# 数据库操作指令

协助用户完成各类数据库相关任务。

## 支持的操作

### SQL 查询编写

```sql
-- 复杂查询：多表 JOIN、子查询、窗口函数、CTE
-- 示例：带分页的聚合查询
WITH page AS (
  SELECT id, title, author_id, created_at,
         ROW_NUMBER() OVER (ORDER BY created_at DESC) AS rn
  FROM articles
  WHERE status = 'published'
)
SELECT p.*, u.name AS author_name
FROM page p
JOIN users u ON u.id = p.author_id
WHERE rn BETWEEN 1 AND 20;
```

### 查询优化

1. 先 `EXPLAIN ANALYZE` 看执行计划
2. 检查全表扫描（Seq Scan）→ 建议索引
3. 检查嵌套循环（Nested Loop）→ 是否可改 JOIN 策略
4. 检查排序操作（Sort）→ 是否可用索引排序
5. 检查临时文件 → 增大 work_mem 或优化查询

### 表结构设计

- 字段类型选择（INT vs BIGINT, VARCHAR vs TEXT, DECIMAL vs FLOAT）
- 索引策略（B-tree, Hash, GIN, GiST, 部分索引, 覆盖索引）
- 范式化 vs 反范式化权衡
- 分区表设计

### 数据迁移

- 编写可回滚的迁移脚本
- 大表迁移用分批处理（batch）
- 零停机迁移策略（影子表 + 触发器）

### 常用诊断

```sql
-- 慢查询
SELECT query, calls, total_time, mean_time, rows
FROM pg_stat_statements
ORDER BY mean_time DESC LIMIT 20;

-- 锁等待
SELECT blocked_locks.pid, blocked_activity.query AS blocked_query
FROM pg_locks blocked_locks
JOIN pg_stat_activity blocked_activity ON blocked_activity.pid = blocked_locks.pid;

-- 表大小
SELECT relname, pg_size_pretty(pg_total_relation_size(relid))
FROM pg_catalog.pg_statio_user_tables
ORDER BY pg_total_relation_size(relid) DESC;
```

## 规则

- 执行 SQL 前先预览（SELECT 先跑 LIMIT 5 确认）
- DDL（ALTER/DROP/TRUNCATE）前必须确认用户意图
- 生产环境修改先建议在测试环境验证
- 解释查询计划时用通俗语言说明
