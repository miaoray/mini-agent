# Mini-Agent 设计规范

## 垂直滚动条 (Vertical Scrollbar)

所有需要垂直滚动的区域统一采用与 Sidebar 会话列表相同的滚动条样式，保持 UI 一致性。

### 规范

| 属性 | 值 | 说明 |
|------|-----|------|
| 宽度 | 6px | 仅垂直滚动条 |
| 轨道 (track) | transparent | 无背景 |
| 滑块 (thumb) 默认 | transparent | 默认隐藏 |
| 滑块 显示时机 | 父容器/自身 hover | 鼠标悬停时显示 |
| 滑块 颜色 (亮色) | #c0c0c0 | 悬停时 |
| 滑块 悬停 (亮色) | #a0a0a0 | 滑块被悬停时加深 |
| 滑块 颜色 (暗色) | #52525b | 暗色主题 |
| 滑块 悬停 (暗色) | #71717a | 暗色主题滑块悬停 |
| 圆角 | 3px | border-radius |
| 角落 | display: none | 隐藏滚动条角落 |

### 应用范围

| 区域 | 容器 | Hover 触发 |
|------|------|------------|
| Sidebar 会话列表 | `.conversation-list` | `.sidebar:hover` |
| 主聊天滚动区 | `.chat-view-scroll` | `.chat-view:hover` |
| Thinking 面板 | `.thinking-panel` | `.thinking-panel:hover` |
| 审批预览 | `.approval-preview` | `.approval-preview:hover` |
| Debug 日志列表 | `.debug-logs` | `.debug-panel:hover` |
| Debug JSON 详情 | `.debug-json` | `.debug-panel:hover` |

### 行为说明

- **大区域**（Sidebar、主聊天区）：鼠标悬停整个区域时显示滚动条
- **小区域**（Thinking、审批预览）：鼠标悬停该滚动容器本身时显示滚动条
- **Debug 面板**：鼠标悬停 Debug 面板时，其内部滚动区域显示滚动条

### 实现参考

- 使用 `::-webkit-scrollbar` 系列伪元素
- 仅支持 WebKit 内核（Chrome、Safari、Edge、Electron/Tauri WebView）
- 其他浏览器回退为系统默认滚动条
