# Hydrate 机制说明

## 是什么

`hydrate`（`hydrateConversationMessages`）负责**从后端 DB 拉取消息，并与前端内存中的消息合并**，作为当前会话的权威数据源，写入 store 的 `messagesByConversation`。

## 触发时机

1. **切换会话**：`currentConversationId` 变化时，`useEffect` 调用 `hydrateConversationMessages(currentConversationId)`
2. **chat-done 后**：`ChatView` 的 `onChatDone` 回调即 `hydrateConversationMessages`，每次收到 `chat-done` 都会触发

## 处理流程

```
1. invoke("list_messages", { conversationId })
   → 从 SQLite 获取该会话的所有消息（按 created_at 排序）

2. dbChatMessages = 转为前端 ChatMessage 格式（id, conversationId, role, content）
   dbIds = 所有 DB 消息的 id 集合

3. memoryMessages = store 中该会话的当前消息
   memoryOnly = memory 中 id 不在 dbIds 里的消息（乐观更新、尚未持久化的）

4. merged = 对每个 dbMsg：
   - 若 memory 中有同 id 且 (content 更长 或 有 thinking) → 用 memory 版本
   - 否则 → 用 db 版本

5. final = merged + memoryOnly（DB 消息在前，memoryOnly 追加）

6. setMessagesForConversation(conversationId, final)
```

## 合并规则

| 情况 | 结果 |
|------|------|
| 消息只在 DB | 使用 DB 版本 |
| 消息只在 memory | 进入 memoryOnly，追加到 final |
| 消息在两者 | 若 memory 的 content 更长或有 thinking，用 memory；否则用 DB |

## 为何需要 hydrate

- **DB 为持久化源**：刷新、重启后消息来自 DB
- **memory 有实时更新**：chat-done 的 `upsertMessage` 先写入 memory，thinking 等 DB 不存的数据也在 memory
- **合并**：以 DB 为主，用 memory 中更新、更完整的内容覆盖，并保留 memoryOnly（如刚发的 user 消息）

## 日志

- `[hydrate-trigger]`：hydrate 被触发
- `[hydrate]`：合并结果（db_count, memory_count, merged_count, assistant_contents 等）
