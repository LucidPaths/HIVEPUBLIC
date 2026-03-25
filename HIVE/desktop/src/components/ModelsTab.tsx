import {
  ChevronDown, ChevronRight, Cloud, Check, Loader2, Key, Eye, EyeOff,
  RefreshCw, FolderOpen, Play, StopCircle, Send
} from 'lucide-react';
import * as api from '../lib/api';
import { Tab, Backend } from '../types';

interface Props {
  // Local models
  models: api.LocalModel[];
  selectedModel: api.LocalModel | null;
  onSelectModel: (model: api.LocalModel) => void;
  serverRunning: boolean;
  serverLoading: boolean;
  onStartModel: () => void;
  onStopModel: () => void;
  onLoadModels: () => void;
  backend: Backend;

  // Cloud providers
  providers: api.ProviderConfig[];
  providerStatuses: Record<string, api.ProviderStatus>;
  showProviders: boolean;
  onToggleProviders: () => void;
  apiKeyInput: Record<string, string>;
  onApiKeyInputChange: (provider: string, value: string) => void;
  showApiKey: Record<string, boolean>;
  onToggleShowApiKey: (provider: string) => void;
  savingKey: string | null;
  onSaveApiKey: (provider: api.ProviderType) => void;
  onRemoveApiKey: (provider: api.ProviderType) => void;

  // Cloud model selection
  selectedCloudModel: { provider: api.ProviderType; model: api.ProviderModel } | null;
  onSelectCloudModel: (provider: api.ProviderType, model: api.ProviderModel) => void;
  activeModelType: 'local' | 'cloud';

  // Navigation
  onSetTab: (tab: Tab) => void;
  onShowModelInfo: () => void;
}

export default function ModelsTab({
  models, selectedModel, onSelectModel, serverRunning, serverLoading,
  onStartModel, onStopModel, onLoadModels, backend,
  providers, providerStatuses, showProviders, onToggleProviders,
  apiKeyInput, onApiKeyInputChange, showApiKey, onToggleShowApiKey,
  savingKey, onSaveApiKey, onRemoveApiKey,
  selectedCloudModel, onSelectCloudModel, activeModelType,
  onSetTab, onShowModelInfo,
}: Props) {
  return (
    <div className="h-full p-6 overflow-auto">
      <div className="max-w-2xl mx-auto">
        {/* Cloud Providers Section */}
        <div className="mb-6">
          <button
            onClick={() => {
              console.log('[HIVE] UI: Cloud Providers section', showProviders ? 'collapsed' : 'expanded');
              onToggleProviders();
            }}
            className="flex items-center gap-2 w-full p-3 bg-zinc-800 hover:bg-zinc-700 rounded-lg text-left"
          >
            {showProviders ? <ChevronDown className="w-5 h-5 text-zinc-400" /> : <ChevronRight className="w-5 h-5 text-zinc-400" />}
            <Cloud className="w-5 h-5 text-amber-400" />
            <span className="text-white font-medium">Cloud Providers</span>
            <span className="ml-auto text-xs text-zinc-500">
              {providers.filter(p => p.provider_type !== 'local' && p.has_api_key).length} configured
            </span>
          </button>

          {showProviders && (
            <div className="mt-3 space-y-3">
              {providers.filter(p => p.provider_type !== 'local').map((provider) => {
                const info = api.getProviderInfo(provider.provider_type);
                const status = providerStatuses[provider.provider_type];

                return (
                  <div key={provider.provider_type} className="p-4 bg-zinc-800/50 rounded-lg border border-zinc-700">
                    <div className="flex items-center gap-3 mb-3">
                      <span className="text-xl">{info.icon}</span>
                      <div className="flex-1">
                        <h4 className={`font-medium ${info.color}`}>{provider.name}</h4>
                        <p className="text-xs text-zinc-500">
                          {status?.connected ? (
                            <span className="text-green-400">Connected - {status.models.length} models</span>
                          ) : status?.error ? (
                            <span className="text-yellow-400">{status.error}</span>
                          ) : provider.has_api_key ? (
                            <span className="text-zinc-400">API key configured</span>
                          ) : (
                            <span className="text-zinc-500">Not configured</span>
                          )}
                        </p>
                      </div>
                      {provider.has_api_key && (
                        <button
                          onClick={() => onRemoveApiKey(provider.provider_type)}
                          disabled={savingKey === provider.provider_type}
                          className="text-xs text-red-400 hover:text-red-300 px-2 py-1 rounded hover:bg-red-500/10"
                        >
                          {savingKey === provider.provider_type ? <Loader2 className="w-3 h-3 animate-spin" /> : 'Remove'}
                        </button>
                      )}
                    </div>

                    {/* API Key Input or Saved Indicator */}
                    {!provider.has_api_key ? (
                      <div className="flex gap-2">
                        <div className="flex-1 relative">
                          <input
                            type={showApiKey[provider.provider_type] ? 'text' : 'password'}
                            value={apiKeyInput[provider.provider_type] || ''}
                            onChange={(e) => onApiKeyInputChange(provider.provider_type, e.target.value)}
                            placeholder={`Enter ${provider.name} API key`}
                            className="w-full bg-zinc-900 text-white px-3 py-2 pr-10 rounded-lg border border-zinc-600 text-sm font-mono"
                          />
                          <button
                            onClick={() => onToggleShowApiKey(provider.provider_type)}
                            className="absolute right-2 top-1/2 -translate-y-1/2 text-zinc-400 hover:text-white"
                          >
                            {showApiKey[provider.provider_type] ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
                          </button>
                        </div>
                        <button
                          onClick={() => onSaveApiKey(provider.provider_type)}
                          disabled={!apiKeyInput[provider.provider_type]?.trim() || savingKey === provider.provider_type}
                          className="px-4 py-2 bg-amber-500 hover:bg-amber-600 disabled:opacity-50 text-black text-sm font-medium rounded-lg flex items-center gap-2"
                        >
                          {savingKey === provider.provider_type ? (
                            <Loader2 className="w-4 h-4 animate-spin" />
                          ) : (
                            <Key className="w-4 h-4" />
                          )}
                          Save
                        </button>
                      </div>
                    ) : (
                      <div className="flex items-center gap-2 p-2 bg-green-500/10 border border-green-500/30 rounded-lg">
                        <Check className="w-4 h-4 text-green-400" />
                        <span className="text-sm text-green-400">API key saved securely</span>
                        <button
                          onClick={() => onRemoveApiKey(provider.provider_type)}
                          className="ml-auto text-xs text-red-400 hover:text-red-300"
                        >
                          Remove
                        </button>
                      </div>
                    )}

                    {/* Available Models - Selectable */}
                    {(status?.connected || provider.has_api_key) && status?.models && status.models.length > 0 && (
                      <div className="mt-3 pt-3 border-t border-zinc-700">
                        <p className="text-xs text-zinc-500 mb-2">Click to select a model:</p>
                        <div className="space-y-1">
                          {status.models.map(model => {
                            const isSelected = selectedCloudModel?.provider === provider.provider_type &&
                                              selectedCloudModel?.model.id === model.id;
                            return (
                              <button
                                key={model.id}
                                onClick={() => {
                                  console.log('[HIVE] UI: Selected cloud model:', provider.provider_type, '/', model.id);
                                  onSelectCloudModel(provider.provider_type, model);
                                }}
                                className={`w-full text-left px-3 py-2 rounded-lg text-sm transition-colors ${
                                  isSelected
                                    ? 'bg-amber-500/20 border border-amber-500 text-amber-400'
                                    : 'bg-zinc-700/50 hover:bg-zinc-700 text-zinc-300 border border-transparent'
                                }`}
                              >
                                <div className="flex items-center justify-between">
                                  <span className="font-medium">{model.name}</span>
                                  {isSelected && <Check className="w-4 h-4" />}
                                </div>
                                {model.description && (
                                  <p className="text-xs text-zinc-500 mt-0.5">{model.description}</p>
                                )}
                              </button>
                            );
                          })}
                        </div>
                      </div>
                    )}
                  </div>
                );
              })}

              <p className="text-xs text-zinc-500 flex items-center gap-2 px-2">
                <Key className="w-3 h-3" />
                API keys are encrypted and stored securely in your system keyring
              </p>
            </div>
          )}
        </div>

        {/* Local Models Section */}
        <div className="flex items-center justify-between mb-6">
          <h2 className="text-xl font-semibold text-white">
            {backend === 'wsl' ? 'Local Models (WSL + Windows)' : 'Local Models'}
          </h2>
          <div className="flex gap-2">
            <button
              onClick={() => onLoadModels()}
              className="flex items-center gap-2 px-3 py-2 text-sm text-zinc-400 hover:text-white hover:bg-zinc-800 rounded-lg"
            >
              <RefreshCw className="w-4 h-4" />
            </button>
            <button
              onClick={() => api.openModelsDirectory()}
              className="flex items-center gap-2 px-3 py-2 text-sm text-zinc-400 hover:text-white hover:bg-zinc-800 rounded-lg"
            >
              <FolderOpen className="w-4 h-4" />
              Open Folder
            </button>
          </div>
        </div>

        {models.length === 0 ? (
          <div className="text-center py-12 text-zinc-500">
            <p className="mb-4">No models found.</p>
            <button
              onClick={() => onSetTab('browse')}
              className="text-amber-400 hover:underline"
            >
              Browse HuggingFace to download models
            </button>
          </div>
        ) : (
          <div className="space-y-3">
            {models.map((model) => (
              <div
                key={model.path}
                onClick={() => {
                  console.log('[HIVE] UI: Selected local model:', model.filename, `(${model.size_gb.toFixed(2)} GB)`);
                  onSelectModel(model);
                  onShowModelInfo();
                }}
                className={`p-4 rounded-xl cursor-pointer transition-all ${
                  selectedModel?.path === model.path && activeModelType === 'local'
                    ? 'bg-amber-500/20 border-2 border-amber-500'
                    : 'bg-zinc-800 border-2 border-transparent hover:border-zinc-600'
                }`}
              >
                <div className="flex items-center justify-between">
                  <div>
                    <h3 className="text-white font-medium">{model.filename}</h3>
                    <p className="text-zinc-400 text-sm">
                      {model.size_gb.toFixed(2)} GB
                      {model.path.startsWith('/') && (
                        <span className="ml-2 text-xs bg-zinc-700 px-2 py-0.5 rounded">WSL</span>
                      )}
                    </p>
                  </div>
                  {selectedModel?.path === model.path && activeModelType === 'local' && (
                    <Check className="w-5 h-5 text-amber-500" />
                  )}
                </div>
              </div>
            ))}
          </div>
        )}

        {/* Action buttons for selected model */}
        {selectedModel && activeModelType === 'local' && (
          <div className="mt-6 flex gap-3">
            {serverRunning ? (
              <button
                onClick={onStopModel}
                className="flex-1 py-3 bg-red-500 hover:bg-red-600 text-white font-medium rounded-xl flex items-center justify-center gap-2"
              >
                <StopCircle className="w-5 h-5" />
                Stop Model
              </button>
            ) : (
              <button
                onClick={onStartModel}
                disabled={serverLoading}
                className="flex-1 py-3 bg-amber-500 hover:bg-amber-600 disabled:opacity-50 text-black font-medium rounded-xl flex items-center justify-center gap-2"
              >
                {serverLoading ? (
                  <Loader2 className="w-5 h-5 animate-spin" />
                ) : (
                  <Play className="w-5 h-5" />
                )}
                {serverLoading ? 'Loading...' : `Load Model (${backend.toUpperCase()})`}
              </button>
            )}
          </div>
        )}

        {/* Action button for cloud model */}
        {selectedCloudModel && activeModelType === 'cloud' && (
          <div className="mt-6">
            <div className="p-4 bg-zinc-800 rounded-xl mb-3">
              <div className="flex items-center gap-3">
                <span className="text-xl">{api.getProviderInfo(selectedCloudModel.provider).icon}</span>
                <div>
                  <p className="text-white font-medium">{selectedCloudModel.model.name}</p>
                  <p className="text-zinc-400 text-sm">
                    {api.getProviderInfo(selectedCloudModel.provider).name}
                    {selectedCloudModel.model.context_length && (
                      <span className="ml-2">• {(selectedCloudModel.model.context_length / 1000).toFixed(0)}K context</span>
                    )}
                  </p>
                </div>
              </div>
            </div>
            <button
              onClick={() => onSetTab('chat')}
              className="w-full py-3 bg-amber-500 hover:bg-amber-600 text-black font-medium rounded-xl flex items-center justify-center gap-2"
            >
              <Send className="w-5 h-5" />
              Start Chat
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
