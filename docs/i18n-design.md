# 多语言支持 (i18n) 架构设计

## 概述

本项目采用 **客户端多语言** 架构，通过 Zustand store 管理语言状态，支持中文 (zh) 和英文 (en) 两种语言。

## 当前实现

### 1. 语言状态管理

**文件**: `src/stores/localeStore.ts`

```typescript
export type Locale = "en" | "zh";

type LocaleState = {
  locale: Locale;
  setLocale: (locale: Locale) => void;
};

export const useLocaleStore = create<LocaleState>()(
  persist(
    (set) => ({
      locale: "en",
      setLocale: (locale) => set({ locale }),
    }),
    { name: LOCALE_KEY } // 持久化到 localStorage
  )
);
```

- 默认语言: `en` (英文)
- 持久化 key: `mini-agent-locale`
- 支持的语言: `en`, `zh`

### 2. 组件中使用多语言

**方式一: 条件渲染**

```tsx
const { locale, setLocale } = useLocaleStore();

// 在 JSX 中使用三元表达式
<span>{locale === "zh" ? "删除会话" : "Delete conversation"}</span>
```

**方式二: 切换语言**

```tsx
<button onClick={() => setLocale(locale === "en" ? "zh" : "en")}>
  {locale === "en" ? "中文" : "EN"}
</button>
```

### 3. 已有多语言支持的组件

| 组件 | 文件 | 说明 |
|------|------|------|
| Sidebar | `src/components/Sidebar.tsx` | 侧边栏切换按钮、删除确认对话框 |
| ChatView | `src/components/ChatView.tsx` | 聊天界面 |
| ConfirmDialog | `src/components/ConfirmDialog.tsx` | 确认对话框 |
| ConfigBanner | `src/components/ConfigBanner.tsx` | 配置提示横幅 (推荐模式: 映射对象) |

## 设计原则

### 1. 字符串管理

**原则**: 每个需要多语言的字符串都应在组件内部使用三元表达式或映射对象管理。

**推荐: 使用映射对象** (适用于多字符串场景)

```tsx
const messages = {
  en: { title: "Delete Conversation", confirm: "Delete", cancel: "Cancel" },
  zh: { title: "删除会话", confirm: "确认删除", cancel: "取消" },
};

// 使用
const { title, confirm, cancel } = messages[locale];
```

**适用场景**:
- 组件有 2 个以上需要翻译的字符串
- 字符串之间有逻辑关联
- 需要保持代码整洁

**示例** (`ConfigBanner.tsx`):

```tsx
const messages = {
  en: {
    message: 'Configure your `MINIMAX_API_KEY` in the `.env` file to get started.',
    linkText: "Get API Key",
  },
  zh: {
    message: '请在 `.env` 文件中配置 `MINIMAX_API_KEY` 以开始使用。',
    linkText: "获取 API Key",
  },
};

const { message, linkText } = messages[locale];
```

### 2. 国际化流程

1. **新增字符串**: 使用 `{locale === "zh" ? "中文" : "English"}` 模式
2. **修改逻辑**: 如果组件已有 `const { locale } = useLocaleStore()`，直接使用
3. **新增组件**: 引入 `useLocaleStore` 并在组件中使用

### 3. 后续扩展

如需支持更多语言:

1. 修改 `Locale` 类型:
   ```typescript
   export type Locale = "en" | "zh" | "ja" | "ko";
   ```

2. 更新 `localeStore.ts` 默认值:
   ```typescript
   locale: "en",
   ```

3. 在各组件中添加对应语言的字符串

## 待完善

- [ ] 考虑提取公共字符串到独立的 `i18n` 目录
- [ ] 添加 `useTranslation` hook 简化多语言实现
- [ ] 添加自动检测浏览器语言功能

## 相关文件

| 文件路径 | 说明 |
|---------|------|
| `src/stores/localeStore.ts` | 语言状态管理 |
| `src/components/Sidebar.tsx` | 多语言示例 - 侧边栏 |
| `src/components/ChatView.tsx` | 多语言示例 - 聊天界面 |
| `src/components/ConfirmDialog.tsx` | 多语言示例 - 对话框 |
| `src/components/ConfigBanner.tsx` | 多语言示例 - 配置提示 |
