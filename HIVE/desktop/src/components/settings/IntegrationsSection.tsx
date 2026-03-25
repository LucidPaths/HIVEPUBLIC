import { useState, useEffect } from 'react';
import { Link, Key, Trash2, ExternalLink } from 'lucide-react';
import * as api from '../../lib/api';
import TelegramDaemonControls from './TelegramDaemonControls';
import DiscordDaemonControls from './DiscordDaemonControls';

interface IntegrationDoor {
  id: api.IntegrationProvider;
  name: string;
  description: string;
  placeholder: string;
  helpUrl: string;
  helpText: string;
}

const INTEGRATION_DOORS: IntegrationDoor[] = [
  {
    id: 'telegram',
    name: 'Telegram Bot',
    description: 'Command HIVE remotely, get notifications, send messages',
    placeholder: '123456789:ABCdefGHIjklMNOpqrSTUvwxYZ',
    helpUrl: 'https://core.telegram.org/bots#botfather',
    helpText: 'Get a token from @BotFather on Telegram',
  },
  {
    id: 'discord',
    name: 'Discord Bot Token',
    description: 'HIVE joins Discord: send messages, read channels, respond to your server',
    placeholder: 'MTxxxxxxxxxxxxxxxxxxxxxxxx.Gxxxxx.xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx',
    helpUrl: 'https://discord.com/developers/applications',
    helpText: 'Create a bot in Discord Developer Portal → Bot → Reset Token',
  },
  {
    id: 'discord_channel_id',
    name: 'Discord Default Channel ID',
    description: 'Default channel for sending/reading messages (right-click channel → Copy ID)',
    placeholder: '1234567890123456789',
    helpUrl: 'https://discord.com/developers/applications',
    helpText: 'Enable Developer Mode in Discord → right-click channel → Copy Channel ID',
  },
  {
    id: 'github',
    name: 'GitHub',
    description: 'Issues, PRs, code review, repo management',
    placeholder: 'ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx',
    helpUrl: 'https://github.com/settings/tokens',
    helpText: 'Create a Personal Access Token with repo scope',
  },
  {
    id: 'brave',
    name: 'Brave Search API (optional)',
    description: 'Fallback web search engine — free tier: 2000 queries/month, no CAPTCHA',
    placeholder: 'BSAxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx',
    helpUrl: 'https://brave.com/search/api/',
    helpText: 'Sign up free at brave.com/search/api → Get API Key',
  },
  {
    id: 'jina',
    name: 'Jina AI API (optional)',
    description: 'Higher rate limits for web search + page reading. Free 10M tokens on signup',
    placeholder: 'jina_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx',
    helpUrl: 'https://jina.ai/api-dashboard/',
    helpText: 'Get a free key at jina.ai for higher rate limits',
  },
];

export default function IntegrationsSection() {
  const [statuses, setStatuses] = useState<Record<string, boolean>>({});
  const [inputValues, setInputValues] = useState<Record<string, string>>({});
  const [statusMsg, setStatusMsg] = useState<{ msg: string; ok: boolean } | null>(null);
  const [showInputFor, setShowInputFor] = useState<string | null>(null);

  useEffect(() => {
    loadStatuses();
  }, []);

  async function loadStatuses() {
    try {
      const s = await api.getIntegrationStatuses();
      setStatuses(s);
    } catch (e) {
      console.error('[HIVE] Failed to load integration statuses:', e);
    }
  }

  async function handleSaveKey(provider: api.IntegrationProvider) {
    const key = inputValues[provider]?.trim();
    if (!key) return;
    try {
      await api.storeIntegrationKey(provider, key);
      setStatuses(prev => ({ ...prev, [provider]: true }));
      setInputValues(prev => ({ ...prev, [provider]: '' }));
      setShowInputFor(null);
      setStatusMsg({ msg: `${provider} key saved and encrypted`, ok: true });
    } catch (e) {
      setStatusMsg({ msg: `Failed: ${e}`, ok: false });
    }
  }

  async function handleDeleteKey(provider: api.IntegrationProvider) {
    try {
      await api.deleteIntegrationKey(provider);
      setStatuses(prev => ({ ...prev, [provider]: false }));
      setStatusMsg({ msg: `${provider} key removed`, ok: true });
    } catch (e) {
      setStatusMsg({ msg: `Failed: ${e}`, ok: false });
    }
  }

  useEffect(() => {
    if (!statusMsg) return;
    const t = setTimeout(() => setStatusMsg(null), 4000);
    return () => clearTimeout(t);
  }, [statusMsg]);

  return (
    <div className="bg-zinc-800 rounded-xl p-6 mt-4">
      <h3 className="text-white font-medium mb-2 flex items-center gap-2">
        <Link className="w-5 h-5" />
        Integrations
      </h3>
      <p className="text-zinc-400 text-sm mb-4">
        Connect HIVE to external services. You provide the key — HIVE provides the door.
        Keys are encrypted with AES-256-GCM on your machine.
      </p>

      {statusMsg && (
        <div className={`text-sm px-3 py-2 rounded-lg mb-3 ${statusMsg.ok ? 'bg-green-500/10 text-green-400' : 'bg-red-500/10 text-red-400'}`}>
          {statusMsg.msg}
        </div>
      )}

      {/* Telegram Daemon Controls */}
      {statuses['telegram'] && <TelegramDaemonControls />}

      {/* Discord Daemon Controls */}
      {statuses['discord'] && <DiscordDaemonControls />}

      <div className="space-y-3">
        {INTEGRATION_DOORS.map(door => {
          const isConnected = statuses[door.id] ?? false;
          const isEditing = showInputFor === door.id;

          return (
            <div key={door.id} className="bg-zinc-900 rounded-lg p-4">
              <div className="flex items-center justify-between mb-1">
                <div className="flex items-center gap-2">
                  <span className={`w-2 h-2 rounded-full ${isConnected ? 'bg-green-400' : 'bg-zinc-600'}`} />
                  <span className="text-white font-medium text-sm">{door.name}</span>
                  {isConnected && <span className="text-xs text-green-400">Connected</span>}
                </div>
                <div className="flex items-center gap-1">
                  {isConnected && (
                    <button
                      onClick={() => handleDeleteKey(door.id)}
                      className="p-1.5 text-zinc-500 hover:text-red-400 transition-colors"
                      title="Remove key"
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </button>
                  )}
                  <button
                    onClick={() => setShowInputFor(isEditing ? null : door.id)}
                    className="p-1.5 text-zinc-500 hover:text-amber-400 transition-colors"
                    title={isConnected ? 'Update key' : 'Add key'}
                  >
                    <Key className="w-3.5 h-3.5" />
                  </button>
                </div>
              </div>
              <p className="text-zinc-500 text-xs">{door.description}</p>

              {isEditing && (
                <div className="mt-3 space-y-2">
                  <div className="flex gap-2">
                    <input
                      type="password"
                      value={inputValues[door.id] || ''}
                      onChange={(e) => setInputValues(prev => ({ ...prev, [door.id]: e.target.value }))}
                      placeholder={door.placeholder}
                      className="flex-1 bg-zinc-800 text-white px-3 py-2 rounded-lg border border-zinc-600 focus:border-amber-500 outline-none text-sm font-mono"
                      onKeyDown={(e) => { if (e.key === 'Enter') handleSaveKey(door.id); }}
                    />
                    <button
                      onClick={() => handleSaveKey(door.id)}
                      disabled={!inputValues[door.id]?.trim()}
                      className="px-3 py-2 text-sm bg-amber-500 hover:bg-amber-600 disabled:bg-zinc-700 disabled:text-zinc-500 text-black rounded-lg font-medium"
                    >
                      Save
                    </button>
                  </div>
                  <a
                    href={door.helpUrl}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-xs text-amber-400/70 hover:text-amber-400 flex items-center gap-1"
                  >
                    <ExternalLink className="w-3 h-3" />
                    {door.helpText}
                  </a>
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
