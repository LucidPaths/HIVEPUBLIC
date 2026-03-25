import { useState, useEffect, useRef } from 'react';
import { Link, Server, Plug, Copy, Check, ChevronDown, ChevronRight, Plus, X, AlertTriangle } from 'lucide-react';
import * as api from '../lib/api';

// ============================================================
// McpTab — Full-page tab for MCP (Model Context Protocol)
//
//   Left panel:  MCP Server mode — HIVE exposes its tools to external clients
//   Right panel: MCP Client — connect to external MCP servers, manage tools
//
// Self-contained: calls api.* directly, no props threading. (P1: Modularity)
// ============================================================

export default function McpTab() {
  return (
    <div className="h-full flex overflow-hidden">
      {/* Left: Server Mode */}
      <div className="w-1/2 border-r border-zinc-700 flex flex-col overflow-hidden">
        <McpServerPanel />
      </div>

      {/* Right: Client Management */}
      <div className="w-1/2 flex flex-col overflow-hidden">
        <McpClientPanel />
      </div>
    </div>
  );
}

// ============================================================
// Left Panel — MCP Server (HIVE as server)
// ============================================================

function McpServerPanel() {
  const [copied, setCopied] = useState(false);
  const [showTools, setShowTools] = useState(false);
  const [tools, setTools] = useState<api.ToolSchema[]>([]);
  const [tunnelUrl, setTunnelUrl] = useState<string | null>(null);
  const [tunnelLoading, setTunnelLoading] = useState(false);
  const [tunnelPort, setTunnelPort] = useState(8080);
  const [tunnelError, setTunnelError] = useState<string | null>(null);
  const [tunnelCopied, setTunnelCopied] = useState(false);
  const [tunnelAcknowledged, setTunnelAcknowledged] = useState(false);
  const copyTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const tunnelCopyTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    let mounted = true;
    api.getAvailableTools().then(t => mounted && setTools(t)).catch(() => {});
    api.tunnelStatus().then(u => mounted && setTunnelUrl(u)).catch(() => {});
    return () => {
      mounted = false;
      if (copyTimeoutRef.current) clearTimeout(copyTimeoutRef.current);
      if (tunnelCopyTimeoutRef.current) clearTimeout(tunnelCopyTimeoutRef.current);
    };
  }, []);

  const configSnippet = `{
  "mcpServers": {
    "hive": {
      "command": "hive-desktop",
      "args": ["--mcp"]
    }
  }
}`;

  function handleCopy() {
    navigator.clipboard.writeText(configSnippet).catch(() => {});
    setCopied(true);
    copyTimeoutRef.current = setTimeout(() => setCopied(false), 2000);
  }

  return (
    <div className="p-4 overflow-y-auto">
      <div className="flex items-center gap-2 mb-4">
        <Server size={18} className="text-amber-400" />
        <h2 className="text-lg font-semibold text-zinc-100">MCP Server Mode</h2>
      </div>

      <p className="text-sm text-zinc-400 mb-4">
        Run HIVE as a headless MCP server so external clients (Claude Code, Cursor, etc.)
        can use all of HIVE's {tools.length}+ tools — memory, web search, file ops, Telegram,
        Discord, and more — through any provider.
      </p>

      {/* How it works */}
      <div className="bg-zinc-800 rounded-xl p-4 border border-zinc-700 mb-4">
        <h3 className="text-sm font-medium text-zinc-200 mb-2">How it works</h3>
        <ol className="text-xs text-zinc-400 space-y-1.5 list-decimal list-inside">
          <li>Launch HIVE with the <code className="text-amber-300 bg-zinc-700 px-1 rounded">--mcp</code> flag</li>
          <li>HIVE starts a headless MCP server on stdio (no GUI)</li>
          <li>All registered HiveTools are exposed as MCP tools</li>
          <li>Any MCP client can discover and call them transparently</li>
        </ol>
      </div>

      {/* Claude Code config */}
      <div className="bg-zinc-800 rounded-xl p-4 border border-zinc-700 mb-4">
        <div className="flex items-center justify-between mb-2">
          <h3 className="text-sm font-medium text-zinc-200">Claude Code Configuration</h3>
          <button
            onClick={handleCopy}
            className="flex items-center gap-1 text-xs text-zinc-400 hover:text-zinc-200 transition-colors"
          >
            {copied ? <Check size={12} className="text-green-400" /> : <Copy size={12} />}
            {copied ? 'Copied' : 'Copy'}
          </button>
        </div>
        <p className="text-xs text-zinc-500 mb-2">
          Add to <code className="text-zinc-400">~/.claude/claude_code_config.json</code>:
        </p>
        <pre className="bg-zinc-900 rounded-lg p-3 text-xs text-zinc-300 font-mono overflow-x-auto border border-zinc-600">
          {configSnippet}
        </pre>
      </div>

      {/* Available tools list */}
      <div className="bg-zinc-800 rounded-xl p-4 border border-zinc-700">
        <button
          onClick={() => setShowTools(!showTools)}
          className="flex items-center gap-2 text-sm font-medium text-zinc-200 w-full"
        >
          {showTools ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
          Available Tools ({tools.length})
        </button>
        {showTools && tools.length > 0 && (
          <div className="mt-3 space-y-1.5 max-h-64 overflow-y-auto">
            {tools.map(t => (
              <div key={t.name} className="flex items-center gap-2">
                <span className={`text-[10px] px-1.5 py-0.5 rounded font-medium ${
                  t.risk_level === 'low' ? 'bg-green-900/40 text-green-400' :
                  t.risk_level === 'medium' ? 'bg-amber-900/40 text-amber-400' :
                  t.risk_level === 'high' ? 'bg-red-900/40 text-red-400' :
                  'bg-red-900/60 text-red-300'
                }`}>
                  {t.risk_level}
                </span>
                <span className="text-xs text-zinc-300 font-mono">{t.name}</span>
                <span className="text-[10px] text-zinc-500 truncate">{t.description}</span>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* P7: Remote Access via Cloudflare Tunnel */}
      <div className="bg-zinc-800 rounded-xl p-4 border border-zinc-700 mt-4">
        <div className="flex items-center gap-2 mb-2">
          <Link size={14} className="text-amber-400" />
          <h3 className="text-sm font-medium text-zinc-200">Remote Access</h3>
        </div>
        <p className="text-xs text-zinc-400 mb-3">
          Expose a local port via Cloudflare Tunnel — no account needed.
          Use this to give remote MCP clients or inference tools access to HIVE.
        </p>

        {/* P6 SECURITY WARNING — always visible when tunnel section is rendered */}
        <div className="flex items-start gap-2 bg-red-900/20 border border-red-800/50 rounded-lg px-3 py-2.5 mb-3">
          <AlertTriangle size={14} className="text-red-400 mt-0.5 flex-shrink-0" />
          <div className="text-xs text-red-300/90 leading-relaxed">
            <span className="font-semibold text-red-300">No authentication.</span>{' '}
            This creates a public URL with zero access control. Anyone who discovers the URL
            gets full unauthenticated access to whatever service is on the tunneled port
            (llama-server inference, MCP tools, etc.).
            Only use on trusted networks or for brief testing.
          </div>
        </div>

        {tunnelUrl ? (
          <div className="space-y-2">
            <div className="flex items-center gap-2 bg-zinc-900 rounded-lg px-3 py-2 border border-green-900/50">
              <div className="w-2 h-2 rounded-full bg-green-400 animate-pulse" />
              <code className="text-xs text-green-300 flex-1 truncate">{tunnelUrl}</code>
              <button
                onClick={() => {
                  navigator.clipboard.writeText(tunnelUrl);
                  setTunnelCopied(true);
                  tunnelCopyTimeoutRef.current = setTimeout(() => setTunnelCopied(false), 2000);
                }}
                className="text-xs text-zinc-400 hover:text-zinc-200"
              >
                {tunnelCopied ? <Check size={12} className="text-green-400" /> : <Copy size={12} />}
              </button>
            </div>
            <button
              onClick={async () => {
                try {
                  await api.tunnelStop();
                  setTunnelUrl(null);
                  setTunnelAcknowledged(false);
                } catch (e) { setTunnelError(`${e}`); }
              }}
              className="text-xs px-3 py-1.5 bg-red-900/30 hover:bg-red-900/50 text-red-400 rounded-lg"
            >
              Stop Tunnel
            </button>
          </div>
        ) : (
          <div className="space-y-2">
            {/* Acknowledgement gate — user must check before starting */}
            <label className="flex items-start gap-2 cursor-pointer">
              <input
                type="checkbox"
                checked={tunnelAcknowledged}
                onChange={(e) => setTunnelAcknowledged(e.target.checked)}
                className="w-3.5 h-3.5 mt-0.5 rounded bg-zinc-700 border-zinc-600 text-amber-500 focus:ring-amber-500/50"
              />
              <span className="text-[11px] text-zinc-400 leading-relaxed">
                I understand this tunnel has no authentication and will expose my local service to the public internet
              </span>
            </label>

            <div className="flex items-center gap-2">
              <label className="text-xs text-zinc-400">Port:</label>
              <input
                type="number"
                min={1}
                max={65535}
                value={tunnelPort}
                onChange={(e) => {
                  const v = Number(e.target.value);
                  if (v >= 1 && v <= 65535) setTunnelPort(v);
                }}
                className="w-20 bg-zinc-900 text-white text-xs px-2 py-1 rounded border border-zinc-600 focus:border-amber-500 outline-none"
              />
              <button
                onClick={async () => {
                  setTunnelLoading(true);
                  setTunnelError(null);
                  try {
                    const url = await api.tunnelStart(tunnelPort);
                    setTunnelUrl(url);
                  } catch (e) { setTunnelError(`${e}`); }
                  setTunnelLoading(false);
                }}
                disabled={tunnelLoading || !tunnelAcknowledged}
                title={!tunnelAcknowledged ? 'Acknowledge the security warning first' : undefined}
                className="text-xs px-3 py-1.5 bg-amber-500 hover:bg-amber-600 text-black rounded-lg disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {tunnelLoading ? 'Starting...' : 'Start Tunnel'}
              </button>
            </div>
            {tunnelError && (
              <p className="text-xs text-red-400 bg-red-900/20 rounded px-2 py-1">{tunnelError}</p>
            )}
            <p className="text-[10px] text-zinc-500">
              Requires <code className="text-zinc-400">cloudflared</code> installed.{' '}
              <a href="https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/"
                 className="text-amber-500/70 hover:text-amber-400 underline" target="_blank" rel="noopener noreferrer">
                Install guide
              </a>
            </p>
          </div>
        )}
      </div>
    </div>
  );
}

// ============================================================
// Right Panel — MCP Client (HIVE consuming external servers)
// ============================================================

function McpClientPanel() {
  const [connections, setConnections] = useState<api.McpConnectionInfo[]>([]);
  const [showAdd, setShowAdd] = useState(false);
  const [name, setName] = useState('');
  const [command, setCommand] = useState('');
  const [args, setArgs] = useState('');
  const [transport, setTransport] = useState<'stdio' | 'http'>('stdio');
  const [url, setUrl] = useState('');
  const [loading, setLoading] = useState(false);
  const [msg, setMsg] = useState<{ text: string; ok: boolean } | null>(null);

  useEffect(() => { refreshConnections(); }, []);

  async function refreshConnections() {
    try {
      const conns = await api.mcpListConnections();
      setConnections(conns);
    } catch { /* ignore */ }
  }

  async function handleConnect() {
    if (!name.trim()) return;
    if (transport === 'stdio' && !command.trim()) return;
    if (transport === 'http' && !url.trim()) return;
    setLoading(true);
    setMsg(null);
    try {
      const config: api.McpServerConfig = {
        name: name.trim(),
        command: transport === 'stdio' ? command.trim() : '',
        args: transport === 'stdio' && args.trim() ? args.trim().split(/\s+/) : [],
        transport,
        ...(transport === 'http' ? { url: url.trim() } : {}),
      };
      const tools = await api.mcpConnect(config);
      setMsg({ text: `Connected: ${tools.length} tools discovered`, ok: true });
      setName(''); setCommand(''); setArgs(''); setUrl('');
      setShowAdd(false);
      await refreshConnections();
    } catch (e) {
      setMsg({ text: String(e), ok: false });
    } finally {
      setLoading(false);
    }
  }

  async function handleDisconnect(serverName: string) {
    try {
      await api.mcpDisconnect(serverName);
      setMsg({ text: `Disconnected: ${serverName}`, ok: true });
      await refreshConnections();
    } catch (e) {
      setMsg({ text: String(e), ok: false });
    }
  }

  return (
    <div className="p-4 overflow-y-auto">
      <div className="flex items-center gap-2 mb-4">
        <Plug size={18} className="text-violet-400" />
        <h2 className="text-lg font-semibold text-zinc-100">External MCP Servers</h2>
      </div>

      <p className="text-sm text-zinc-400 mb-4">
        Connect to external MCP servers to expand HIVE's toolset. Community servers,
        custom tools, and third-party services appear alongside built-in tools — the
        model sees one unified list.
      </p>

      {/* Active connections */}
      {connections.length > 0 ? (
        <div className="space-y-3 mb-4">
          {connections.map(conn => (
            <div key={conn.name} className="bg-zinc-800 rounded-xl p-4 border border-zinc-700">
              <div className="flex items-center justify-between mb-2">
                <div className="flex items-center gap-2">
                  <div className="w-2 h-2 rounded-full bg-green-400 animate-pulse" />
                  <span className="text-sm text-zinc-200 font-medium">{conn.name}</span>
                  <span className="text-xs text-zinc-500 font-mono">{conn.url || conn.command}</span>
                  <span className="text-[10px] text-zinc-600 bg-zinc-700 px-1 rounded">{conn.transport || 'stdio'}</span>
                </div>
                <div className="flex items-center gap-2">
                  <span className="text-xs text-green-400">{conn.tools.length} tools</span>
                  <button
                    onClick={() => handleDisconnect(conn.name)}
                    className="flex items-center gap-1 text-xs text-red-400 hover:text-red-300 px-2 py-1 rounded bg-zinc-700 hover:bg-zinc-600 transition-colors"
                  >
                    <X size={10} />
                    Disconnect
                  </button>
                </div>
              </div>
              {conn.tools.length > 0 && (
                <div className="flex flex-wrap gap-1">
                  {conn.tools.slice(0, 12).map(t => (
                    <span key={t} className="text-[10px] bg-zinc-700 text-zinc-400 px-1.5 py-0.5 rounded font-mono">
                      {t}
                    </span>
                  ))}
                  {conn.tools.length > 12 && (
                    <span className="text-[10px] text-zinc-500">+{conn.tools.length - 12} more</span>
                  )}
                </div>
              )}
            </div>
          ))}
        </div>
      ) : (
        <div className="bg-zinc-800/50 rounded-xl p-6 border border-zinc-700/50 mb-4 text-center">
          <Link size={24} className="mx-auto text-zinc-600 mb-2" />
          <p className="text-sm text-zinc-500">No external MCP servers connected</p>
          <p className="text-xs text-zinc-600 mt-1">Connect a server to expand HIVE's capabilities</p>
        </div>
      )}

      {/* Add server form */}
      {showAdd ? (
        <div className="bg-zinc-800 rounded-xl p-4 border border-violet-500/30 space-y-3">
          <h3 className="text-sm font-medium text-zinc-200">Connect MCP Server</h3>

          {/* Transport toggle */}
          <div className="flex gap-1 bg-zinc-900 rounded-lg p-0.5">
            <button
              onClick={() => setTransport('stdio')}
              className={`flex-1 text-xs py-1.5 rounded-md transition-colors ${transport === 'stdio' ? 'bg-violet-600 text-white' : 'text-zinc-400 hover:text-zinc-200'}`}
            >
              Local Process (stdio)
            </button>
            <button
              onClick={() => setTransport('http')}
              className={`flex-1 text-xs py-1.5 rounded-md transition-colors ${transport === 'http' ? 'bg-violet-600 text-white' : 'text-zinc-400 hover:text-zinc-200'}`}
            >
              Remote URL (HTTP)
            </button>
          </div>

          <input
            type="text"
            value={name}
            onChange={e => setName(e.target.value)}
            placeholder="Server name (e.g. filesystem)"
            className="w-full bg-zinc-900 text-zinc-200 rounded-lg px-3 py-2 text-sm border border-zinc-600 focus:border-violet-500 outline-none"
          />

          {transport === 'stdio' ? (
            <>
              <input
                type="text"
                value={command}
                onChange={e => setCommand(e.target.value)}
                placeholder="Command (e.g. npx)"
                className="w-full bg-zinc-900 text-zinc-200 rounded-lg px-3 py-2 text-sm border border-zinc-600 focus:border-violet-500 outline-none"
              />
              <input
                type="text"
                value={args}
                onChange={e => setArgs(e.target.value)}
                placeholder="Arguments (space-separated, e.g. -y @modelcontextprotocol/server-filesystem /home)"
                className="w-full bg-zinc-900 text-zinc-200 rounded-lg px-3 py-2 text-sm border border-zinc-600 focus:border-violet-500 outline-none"
              />
            </>
          ) : (
            <input
              type="text"
              value={url}
              onChange={e => setUrl(e.target.value)}
              placeholder="Server URL (e.g. http://localhost:3001/mcp)"
              className="w-full bg-zinc-900 text-zinc-200 rounded-lg px-3 py-2 text-sm border border-zinc-600 focus:border-violet-500 outline-none"
            />
          )}

          {/* Example servers */}
          <div className="bg-zinc-900/50 rounded-lg p-3 border border-zinc-700">
            <p className="text-[10px] text-zinc-500 uppercase tracking-wider mb-2">Example servers</p>
            <div className="space-y-1.5">
              <button
                onClick={() => { setTransport('stdio'); setName('filesystem'); setCommand('npx'); setArgs('-y @modelcontextprotocol/server-filesystem /home'); }}
                className="block w-full text-left text-xs text-zinc-400 hover:text-zinc-200 transition-colors"
              >
                <span className="text-violet-400">filesystem</span> — read/write files via MCP (stdio)
              </button>
              <button
                onClick={() => { setTransport('stdio'); setName('brave-search'); setCommand('npx'); setArgs('-y @modelcontextprotocol/server-brave-search'); }}
                className="block w-full text-left text-xs text-zinc-400 hover:text-zinc-200 transition-colors"
              >
                <span className="text-violet-400">brave-search</span> — web search via Brave API (stdio)
              </button>
              <button
                onClick={() => { setTransport('stdio'); setName('github'); setCommand('npx'); setArgs('-y @modelcontextprotocol/server-github'); }}
                className="block w-full text-left text-xs text-zinc-400 hover:text-zinc-200 transition-colors"
              >
                <span className="text-violet-400">github</span> — GitHub repository tools (stdio)
              </button>
            </div>
          </div>

          <div className="flex gap-2">
            <button
              onClick={handleConnect}
              disabled={loading || !name.trim() || (transport === 'stdio' ? !command.trim() : !url.trim())}
              className="px-4 py-2 text-sm bg-violet-600 hover:bg-violet-500 text-white rounded-lg disabled:opacity-50 transition-colors"
            >
              {loading ? 'Connecting...' : 'Connect'}
            </button>
            <button
              onClick={() => { setShowAdd(false); setName(''); setCommand(''); setArgs(''); setUrl(''); setTransport('stdio'); }}
              className="px-4 py-2 text-sm bg-zinc-700 hover:bg-zinc-600 text-zinc-300 rounded-lg transition-colors"
            >
              Cancel
            </button>
          </div>
        </div>
      ) : (
        <button
          onClick={() => setShowAdd(true)}
          className="flex items-center gap-2 text-sm text-violet-400 hover:text-violet-300 transition-colors"
        >
          <Plus size={14} />
          Add MCP Server
        </button>
      )}

      {msg && (
        <p className={`text-xs mt-3 ${msg.ok ? 'text-green-400' : 'text-red-400'}`}>{msg.text}</p>
      )}
    </div>
  );
}
