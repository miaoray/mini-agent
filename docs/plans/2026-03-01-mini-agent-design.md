# Mini-Agent 设计文档

> **版本**: 2026-03-01
> **状态**: 设计完成，待实现

---

## 1. 概述

### 1.1 目标

构建一个轻量级桌面 Agent 应用，支持与 LLM 对话、网络检索、资料获取、文件操作（经用户确认）等功能。

### 1.2 核心特性

- **多会话**：支持多个独立对话
- **流式输出**：打字机效果
- **原生 Tools**：web_search、fetch_url、create_directory、write_file
- **用户确认**：Cursor 风格，写文件/建目录前 Accept/Reject
- **多 Provider**：优先 MiniMax M2.5，可切换其他 LLM（Anthropic/OpenAI 兼容）
- **无 Auth**：单机本地应用，后续可扩展

---

## 2. 技术栈

| 层级 | 技术选型 |
|------|----------|
| 桌面壳 | Tauri 2 |
| 前端 | React + TypeScript |
| 状态 | Zustand 或 Context |
| 本地存储 | SQLite |
| LLM 调用 | Anthropic SDK / OpenAI SDK（base_url 兼容） |
| 配置 | .env（API Key 等） |

---

## 3. 领域模型（E-R）

### 3.1 实体关系图

```
Provider (1) ────── (N) Conversation
    │                      │
    │                      │ (1)
    │                      ▼
    │               Message (N)
    │                      │
    │                      │ (1)
    │                      ▼
    └──────────────► AgentTurn (1) ────── (N) ToolInvocation ──────► Tool
                           │
                           │ (1)
                           ▼
                    PendingApproval (N)
```

### 3.2 实体定义

| 实体 | 属性 | 说明 |
|------|------|------|
| **Provider** | id, name, type, base_url, api_key_ref, model_id | LLM 提供方配置 |
| **Conversation** | id, title, provider_id, created_at, updated_at | 会话，预留 user_id |
| **Message** | id, conversation_id, role, content, created_at | 消息 |
| **AgentTurn** | id, message_id, provider_id, prompt_tokens, completion_tokens | 单次推理轮次 |
| **Tool** | id, name, description, schema, impl_ref | 工具定义 |
| **ToolInvocation** | id, tool_id, turn_id, arguments, result, status | 工具调用记录 |
| **PendingApproval** | id, conversation_id, turn_id, action_type, payload, status | 待用户确认操作 |

---

## 4. 架构分层

```
┌─────────────────────────────────────────────────────────────────┐
│  UI Layer (React)                                                │
│  - 会话列表、对话区、消息渲染、流式展示、Accept/Reject            │
├─────────────────────────────────────────────────────────────────┤
│  Agent Orchestrator                                              │
│  - 对话管理、Tool 调度、流式转发、PendingApproval 生命周期         │
├─────────────────────────────────────────────────────────────────┤
│  Tool Registry (Native first, MCP later)                         │
│  - web_search, fetch_url, create_directory, write_file           │
├─────────────────────────────────────────────────────────────────┤
│  LLM Provider Abstraction                                        │
│  - MiniMax M2.5, OpenAI, Anthropic 等统一接口                     │
├─────────────────────────────────────────────────────────────────┤
│  Storage (SQLite)                                                │
└─────────────────────────────────────────────────────────────────┘
```

---

## 5. 功能规格

### 5.1 用户确认流程（Cursor 风格）

| 操作类型 | 需要确认 |
|----------|----------|
| 读文件、检索、fetch（只读） | 否 |
| create_directory | 是，展示路径，Accept/Reject |
| write_file | 是，展示路径 + 内容预览/diff，Accept/Reject |
| 终端命令（若有） | 是 |

UI：每个待确认操作展示为卡片，含操作类型、路径、内容预览，以及 Accept / Reject 按钮；支持逐项或批量处理。

### 5.2 Web Search

- **实现**：独立 Tool，不依赖 LLM 自带检索
- **测试阶段**：免费方案（DuckDuckGo / SearXNG）
- **Token 控制**：返回 3–5 条简洁摘要，每条 ~150 字，避免原始长文注入

### 5.3 流式输出

- LLM 响应以 SSE 或 WebSocket 流式返回
- 前端逐 token 追加渲染

### 5.4 配置

- API Key 存 `.env`，如 `MINIMAX_API_KEY`
- 提供 `.env.example`，不提交 `.env`

---

## 6. 扩展预留

| 扩展项 | 预留方式 |
|--------|----------|
| Auth | 表预留 user_id，查询支持按 user 过滤 |
| MCP | Tool 抽象支持 NativeToolImpl / MCPToolImpl |
| 多 Provider 配置 | Provider 表 + 配置驱动 |

---

## 7. 用户验证场景（Validation Scenarios）

### 7.1 主流程 (P0)

| ID | 场景 | 验收标准 |
|----|------|----------|
| S1.1 | 单轮纯文本对话 | Agent 流式返回，可完整阅读 |
| S1.2 | 多轮对话上下文 | 追问能正确使用上文 |
| S1.3 | 多会话切换 | 各会话历史独立 |
| S2.1 | Agent 调用 web_search | 基于搜索结果回答 |
| S2.2 | 搜索结果简洁 | 工具返回控制 token |
| S3.1 | fetch 网页并总结 | 能拉取并简要总结 |
| S3.2 | 下载文件到指定路径 | 确认后写入正确 |
| S4.1 | create_directory 确认 | 展示路径，Accept/Reject |
| S4.2 | write_file 确认 | 展示 diff，Accept/Reject |
| S5.1 | 流式打字机效果 | 逐 token 显示 |
| S7.1 | 重启保留历史 | 会话和消息完整保留 |

### 7.2 边界与异常 (P1)

| ID | 场景 | 验收标准 |
|----|------|----------|
| S2.3 | 搜索失败 | 有提示，不崩溃 |
| S3.3 | fetch 失败 | 错误信息给 Agent |
| S4.3 | 批量操作确认 | 可逐项或整体处理 |
| S5.2 | 流式 + Tool | Tool 返回后流式续写 |
| S6.1 | API Key 缺失 | 明确引导配置 |
| S6.2 | API 限流/超时 | 有错误信息，可重试 |
| S6.3 | 切换 Provider | 实际调用所选 Provider |
| S7.2 | 新建会话 | 数据与其它会话隔离 |

---

## 8. 非功能需求

- **单用户**：无登录，本地数据
- **并发**：同一时刻仅当前会话跑 Agent
- **错误**：友好提示，关键操作可重试

---

## 9. Tauri 事件监听与 React Strict Mode

### 9.1 问题

React 18 Strict Mode 在开发环境下会 **double-invoke** `useEffect`（mount → unmount → mount），用于检测副作用。若 `setupTauriListeners` 在 `useEffect` 中注册 Tauri 事件，且为异步，会导致：

- 第一次 setup 尚未完成即 unmount
- 第二次 setup 开始，teardown 清理
- 第一次 setup 的异步流程可能仍在执行，继续往 `unlisteners` 中 push
- **同一事件存在两个 listener**，每次 emit 触发两次（如 chat-thinking 收到 18 次、后端发送 9 次）

### 9.2 解决方案

1. **setup 前先 teardown**：每次 `setupTauriListeners` 开头调用 `teardownTauriListeners()`，移除已有 listener，再注册新的。
2. **listenerGeneration 防竞态**：
   - 每次 setup 开始时 `myGen = ++listenerGeneration`
   - 每次 `await listen(...)` 后检查 `myGen !== listenerGeneration`
   - 若已被新一轮 setup 覆盖，则不再 push，避免旧 setup 在 teardown 之后继续注册

3. **提前 return 时必须 unlisten**：当 `myGen !== listenerGeneration` 时，已通过 `await listen()` 注册的 listener 尚未 push 到 `unlisteners`，若直接 return 会变成**孤儿 listener**（永远留在 Tauri 中）。必须在 return 前调用所有已获得的 `unlisten()`，确保从 Tauri 移除。

### 9.3 实现要点

```typescript
let listenerGeneration = 0;

export async function setupTauriListeners() {
  teardownTauriListeners();
  const myGen = ++listenerGeneration;
  const unlisten1 = await listen("chat-thinking", handler);
  if (myGen !== listenerGeneration) {
    unlisten1();  // 必须调用，避免孤儿 listener
    return;
  }
  unlisteners.push(unlisten1);
  // ... 后续 listen 同理，return 前调用所有已注册的 unlisten
}
```

### 9.4 验收

- 前端 RECV 次数与后端 EMIT 次数一致
- 同一事件不会触发两次 handler

---

## 10. 下一步

1. 本设计文档已定稿
2. 调用 **writing-plans** 技能，生成分阶段实现计划（任务拆分、依赖、测试）
3. 按计划执行实现
