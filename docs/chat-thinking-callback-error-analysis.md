# chat-thinking 处理与 "Couldn't find callback id" 错误分析

## 完整处理流程

### 1. 后端发送 chat-thinking

```
run_agent_turn (Rust, tokio::spawn 异步)
  → LLM 返回 result.thinking
  → app_handle.emit("chat-thinking", { conversation_id, message_id, thinking })
```

### 2. 前端接收 chat-thinking

```
ChatView useEffect 内 listen("chat-thinking", callback)
  → callback 执行:
     1. setActiveThinking(thinking)  // Zustand store 更新
     2. console.debug("[chat-thinking] RECV ...")
```

### 3. 关联的异步与副作用链（问题根源）

```
setActiveThinking(thinking)
  → Zustand store 更新
  → 所有 useConversationStore 的组件 re-render
  → App 也订阅了 store（通过 setConversations 等）
  → App re-render
  → App 中: <ChatView onChatDone={(id) => void hydrateConversationMessages(id)} />
  → 每次 render 创建新的内联函数，onChatDone 引用变化
  → ChatView useEffect 依赖 [onChatDone, ...]
  → onChatDone 变化 → useEffect cleanup 执行
  → unlistenThinking(), unlistenDone(), unlistenError() 等
  → 所有 Tauri 事件监听器被注销（callback 从 Tauri 内部移除）
  → 随后 useEffect 再次执行，重新注册监听器（新的 callback id）
```

### 4. 竞态：后端发送 chat-done

```
run_agent_turn 继续执行
  → 处理完成，app_handle.emit("chat-done", { ... })
  → Tauri 尝试投递到前端 callback
  → 若此时前端正在 cleanup 或刚完成 cleanup：
     - 旧的 callback id 已注销
     - 新的 listener 可能尚未注册完成
  → [TAURI] Couldn't find callback id XXXXX
```

## 根因总结

**chat-thinking 的 `setActiveThinking` 触发 App 重渲染 → `onChatDone` 引用变化 → ChatView 的 useEffect 执行 cleanup → 注销 Tauri 监听器 → 后端随后 emit chat-done 时找不到 callback。**

不是页面 HMR 重载，而是 **React 重渲染导致 useEffect 的 deps 变化，进而注销并重建监听器**，在重建过程中与后端的 chat-done 产生竞态。

## 修复方案（已实施）

采用 EventBridge 架构，将 Tauri 事件监听与 React 生命周期解耦：

1. **eventBridge.ts**：应用启动时 `setupTauriListeners()` 注册所有监听器，与 React 无关
2. **lib/conversationHydrate.ts**：抽离 hydrate 逻辑，chat-done 回调直接调用
3. **App.tsx**：`useEffect(() => { setupTauriListeners(); return teardownTauriListeners; }, [])` 仅挂载/卸载时执行
4. **ChatView**：移除所有 Tauri 监听相关代码，不再依赖 onChatDone 等 props

监听器在应用运行期间保持稳定，从根本上消除 "Couldn't find callback id"。
