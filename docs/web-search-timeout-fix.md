# web_search 超时修复说明

## 根因

1. **duckduckgo_search SDK** 使用 `https://api.duckduckgo.com/`，其 reqwest 客户端**未设置 timeout**，请求可能长时间挂起
2. **原流程**：先调 SDK，若返回空再调 HTML fallback。SDK 慢时，外层 10s 超时先触发，从未有机会执行 HTML fallback
3. **HTML fallback** 使用 `duckduckgo.com/html/`，我们可控制其 8s 超时，通常响应更快

## 修复

1. **优先使用 HTML fallback**：先请求 `html.duckduckgo.com/html/`（8s 超时），成功则直接返回
2. **SDK 作为 fallback**：仅当 HTML 失败或返回空时再调 SDK
3. **总超时**：从 10s 提高到 15s
4. **端点**：改用 `html.duckduckgo.com` 作为静态 HTML 入口

## 验证

- 单元测试：`cargo test web_search`
- 集成测试（需网络）：`cargo test web_search_live -- --ignored`
