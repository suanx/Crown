# Coco — 打包构建

本文件夹用于辅助本地打包，不存放正式发布产物。

## 构建步骤

### 前置条件
- Node.js 18+
- Rust toolchain (rustup)
- cargo tauri-cli (`cargo install tauri-cli`)

### 构建命令

```bash
# 1. 构建前端
cd frontend
npm run build

# 2. 构建 Tauri 安装包
cd ../crates/app
cargo tauri build

# 产物在 target/release/bundle/
# 复制到本文件夹:
#   build/installers/  — .msi / .exe 安装包
```

## 产物位置

| 类型 | 路径 |
|------|------|
| MSI 安装包 | `build/installers/*.msi` |
| NSIS 安装包 | `build/installers/*.exe` |
| 便携版 | `build/portable/Coco.exe` |

## 用户使用流程

1. 安装应用
2. 启动 Coco
3. 进入 设置 → Provider → 填入 DeepSeek API Key
4. 点击"测试连接"确认
5. 点击"保存配置"
6. 返回对话页开始使用
