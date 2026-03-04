# Assistant 内容显示调试指南

当后台日志显示 final content 但前端不显示时，按以下步骤定位根因。

## 运行方式

```bash
npm run tauri dev
```

- **后端日志**：终端输出
- **前端日志**：浏览器 DevTools → Console（F12）

## 日志链路

### 1. 后端 emit（终端）

```
[mini-agent][run_agent_turn] emit chat-delta conv=xxx msg=yyy delta_len=...
[mini-agent][run_agent_turn] emit chat-done conv=xxx msg=yyy content_len=... preview="..."
```

- 若出现：说明后端已发出事件
- 若缺失：问题在后端或 emit 失败

### 2. 前端 chat-delta（Console）

```
[chat-delta] conv= ... msg= ... delta_len= ... activeConv= ... activeMsg= ... match= true/false
```

- `match=false`：`activeConversationId` 或 `activeMessageId` 与事件不匹配，delta 被丢弃
- `match=true`：delta 会写入 store

### 3. 前端 chat-done（Console）

```
[chat-done] conv= ... msg= ... content_len= ... activeConv= ... activeMsg= ... match= true/false payload_keys= [...] content_preview= ...
```

- `match=false`：事件被过滤，不会调用 `replaceDelta`
- `content_len=0` 或 `payload_keys` 无 `content`：payload 可能序列化异常
- `[chat-done] calling replaceDelta`：会更新 store
- `[chat-done] skip replaceDelta`：content 为空或缺失

### 4. replaceDelta（Console）

```
[replaceDelta] conv= ... msg= ... content_len= ... existing_index= ... existing_count= ...
```

- `existing_index=-1`：消息不存在，会新建
- `existing_index>=0`：会更新已有消息

### 5. hydrate（Console）

```
[hydrate] conv= ... db_count= ... memory_count= ... merged_count= ... assistant_contents= [{id, len}, ...]
```

- `assistant_contents` 中 `len=0`：hydrate 可能用空内容覆盖了 store
- 若在 chat-done 之后立即出现且 `len=0`：可能是 hydrate 竞态覆盖

### 6. MessageList（Console）

```
[MessageList] assistant content missing: [{id, len}, ...]
```

- 出现：说明渲染时 assistant 消息 content 为空

## 常见根因

| 现象 | 可能原因 |
|------|----------|
| chat-done `match=false` | `activeConversationId`/`activeMessageId` 在 chat-done 前被清空或变更 |
| chat-done `content_len=0` | Tauri 序列化问题或 payload 结构异常 |
| replaceDelta 被调用但 MessageList 仍空 | hydrate 在 replaceDelta 之后用空内容覆盖 |
| hydrate `assistant_contents` 有 len=0 | DB 返回空，或 merge 逻辑未正确保留内存中的 content |
| `[TAURI] Couldn't find callback id XXX` | **页面在 agent 异步运行期间被重载**（如 Vite HMR），导致 emit 的 callback 失效；两次告警通常对应 chat-delta 和 chat-done 两次 emit |

### 关于 callback 告警的恢复

当出现 callback 告警时，emit 的 chat-delta/chat-done 已丢失，但**后端在 emit 前已将 assistant 内容写入 DB**。恢复方式：

1. **lastConversationId 持久化**：App 会将当前会话 ID 存入 localStorage，重载后自动恢复该会话
2. **hydrate 从 DB 拉取**：恢复会话后触发 `hydrateConversationMessages`，从 DB 拉取完整消息（含 assistant 内容）
3. **开发时建议**：agent 运行期间避免保存会触发 Vite 全量重载的文件（如 index.html、vite.config、.env）

## 排查顺序

1. 确认后端 `emit chat-done` 有 `content_len>0`
2. 确认前端 `[chat-done]` 中 `match=true` 且 `content_len>0`
3. 确认出现 `[chat-done] calling replaceDelta`
4. 确认 `[replaceDelta]` 被调用且 `content_len>0`
5. 检查 `[hydrate]` 是否在 chat-done 之后执行，且 `assistant_contents` 中是否有 `len>0`
6. 若仍不显示，检查 `[MessageList]` 是否出现 "assistant content missing"
