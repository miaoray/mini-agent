type ConfigBannerProps = {
  hasApiKey: boolean;
};

function ConfigBanner({ hasApiKey }: ConfigBannerProps) {
  if (hasApiKey) {
    return null;
  }

  return (
    <div className="config-banner" role="alert">
      Missing `MINIMAX_API_KEY`. Configure it in your `.env` file (see `.env.example`).
    </div>
  );
}

export default ConfigBanner;
