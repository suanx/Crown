---
name: api-tester
description: API 接口测试与调试 — 发送 HTTP 请求、检查响应、编写测试用例、生成 API 文档和 Mock 服务。
metadata:
  display-name: API 测试
  icon: "🔌"
  usage: "/api-test <endpoint> 或在对话中描述测试需求"
---

# API 测试指令

协助用户测试和调试 API 接口。

## 快速测试

```bash
# 使用 curl 通用测试模板
curl -s -w "\n\n---\nHTTP %{http_code} | %{time_total}s | %{size_download}bytes\n" \
  -X GET "https://api.example.com/v1/users" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json"
```

## 测试场景

### 功能测试
- 正常请求 → 200 + 正确响应体
- 缺少必填参数 → 400
- 无效认证 → 401
- 无权限 → 403
- 资源不存在 → 404
- 请求体过大 → 413

### 性能测试
```bash
# 简单压测（需要 ab 或 hey）
hey -n 100 -c 10 -H "Authorization: Bearer $TOKEN" \
  -m POST -d '{"query":"test"}' \
  "https://api.example.com/v1/search"
```

### 契约测试
- 检查响应字段类型和格式
- 检查必填字段是否存在
- 检查枚举值合法性
- 检查分页格式

## API 文档生成

从代码注释或 OpenAPI/Swagger 规范生成文档：
- 读取 `openapi.yaml` 或 `swagger.json`
- 验证接口定义与实现的匹配
- 生成 Markdown 格式的接口文档

## Mock 服务

```bash
# 使用 json-server 快速 Mock
npm install -g json-server 2>/dev/null
cat > db.json << 'EOF'
{
  "users": [{"id": 1, "name": "测试用户"}],
  "posts": []
}
EOF
json-server --watch db.json --port 3000
```

## 规则

- 测试前先确认目标环境（dev/staging/prod）
- 涉及修改操作的测试（POST/PUT/DELETE）先在测试环境执行
- 测试完成后整理测试报告
- 发现 bug 时截图或记录完整请求/响应
