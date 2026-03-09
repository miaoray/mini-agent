import { useLocaleStore } from "../stores/localeStore";

type ConfigBannerProps = {
  hasApiKey: boolean;
};

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

function ConfigBanner({ hasApiKey }: ConfigBannerProps) {
  const locale = useLocaleStore((state) => state.locale);

  if (hasApiKey) {
    return null;
  }

  const { message, linkText } = messages[locale];

  return (
    <div className="config-banner" role="alert">
      <span>{message}</span>
      <a href="https://platform.minimaxi.com/" target="_blank" rel="noopener noreferrer">
        {linkText}
      </a>
    </div>
  );
}

export default ConfigBanner;
