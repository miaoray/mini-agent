import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";

type Provider = {
  id: string;
  name: string;
  modelId: string;
  isConfigured: boolean;
};

interface ProviderSelectorProps {
  onProviderChange?: (provider: Provider) => void;
}

export default function ProviderSelector({ onProviderChange }: ProviderSelectorProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [providers, setProviders] = useState<Provider[]>([]);
  const [currentProvider, setCurrentProvider] = useState<Provider | null>(null);
  const [loading, setLoading] = useState(true);
  const dropdownRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    loadProviders();
  }, []);

  // Close dropdown when clicking outside
  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (dropdownRef.current && !dropdownRef.current.contains(event.target as Node)) {
        setIsOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  async function loadProviders() {
    try {
      const list = await invoke<Provider[]>("list_providers");
      setProviders(list);

      const defaultProvider = await invoke<Provider>("get_default_provider");
      setCurrentProvider(defaultProvider);
    } catch (error) {
      console.error("Failed to load providers:", error);
    } finally {
      setLoading(false);
    }
  }

  async function handleSelect(provider: Provider) {
    // If provider is not configured, show warning
    if (!provider.isConfigured) {
      const confirmed = window.confirm(
        `${provider.name} 的 API Key 未配置，是否仍要切换？\n\n请在 .env 文件中设置 ${provider.id.toUpperCase()}_API_KEY`
      );
      if (!confirmed) return;
    }

    try {
      const updated = await invoke<Provider>("set_default_provider", { providerId: provider.id });
      setCurrentProvider(updated);
      setIsOpen(false);
      onProviderChange?.(updated);
    } catch (error) {
      console.error("Failed to set default provider:", error);
    }
  }

  if (loading || !currentProvider) {
    return (
      <div className="provider-selector provider-selector-loading">
        <span className="provider-selector-name">...</span>
      </div>
    );
  }

  // Map provider IDs to display names
  const displayName = currentProvider.id === "minimax" ? "MiniMax M2.5" :
                      currentProvider.id === "deepseek" ? "DeepSeek V3.2" :
                      currentProvider.name;

  return (
    <div className="provider-selector" ref={dropdownRef}>
      <button
        type="button"
        className="provider-selector-trigger"
        onClick={() => setIsOpen(!isOpen)}
        aria-expanded={isOpen}
        aria-haspopup="listbox"
      >
        <span className="provider-selector-name">{displayName}</span>
        <svg
          className={`provider-selector-arrow ${isOpen ? "provider-selector-arrow-open" : ""}`}
          width="12"
          height="12"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <polyline points="6 9 12 15 18 9" />
        </svg>
      </button>

      {isOpen && (
        <div className="provider-dropdown" role="listbox">
          {providers.map((provider) => {
            const displayName = provider.id === "minimax" ? "MiniMax M2.5" :
                              provider.id === "deepseek" ? "DeepSeek V3.2" :
                              provider.name;
            const isActive = provider.id === currentProvider.id;
            return (
              <button
                key={provider.id}
                className={`provider-dropdown-item ${
                  isActive ? "provider-dropdown-item-active" : ""
                }`}
                onClick={() => handleSelect(provider)}
                role="option"
                aria-selected={isActive}
              >
                <span className="provider-dropdown-item-name">{displayName}</span>
                {isActive && <span className="provider-dropdown-check">✓</span>}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
