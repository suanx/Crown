# Crown Frontend

Crown 桌面端正式前端，默认作为 `Crown` Tauri 应用的 WebView UI 运行，也支持浏览器 mock 模式开发。

## 技术栈

- React 18 + TypeScript
- Tailwind CSS 3（全程通过 CSS 变量驱动主题）
- Zustand（状态 + 极简 router）
- Phosphor Icons（精品 SVG icon）
- Vite 6（dev server）

## 启动

```bash
npm install
npm run dev
```

打开 http://localhost:5180

Tauri 桌面壳位于 `../crates/app`，其 `tauri.conf.json` 已指向本目录的 dev server 和 `dist` 构建产物。

## 设计原则

1. **CSS Grid 为外壳，Flex 为内部** — 不用绝对定位拼版面
2. **滚动严格限定在容器内** — body 不滚，每个滚动区域独立
3. **CSS 变量驱动主题** — `.dark` 切换变量，组件不写两套 class
4. **feature-folder 架构** — 按业务能力组织，不按文件类型
5. **每个组件 < 200 行** — 超出强制拆分

## 路由（自做）

```
/welcome       — 欢迎页（空会话状态）
/chat/:id      — 对话页（核心，覆盖所有消息状态）
/settings      — 设置页（双层 Tab 全屏）
```

通过 `useRouterStore` 读写，无 react-router 依赖。

## 颜色基调

- 暖灰底色 `#1F1F1E`（参考 Claude）
- DeepSeek 蓝 `#4D6BFE` 仅用于品牌点缀（按钮、链接、选中态）
- 暗色优先，亮色作为可切换备选
