import {
  RefreshCw, Cpu, Monitor, Check, X, Download, Settings, Loader2
} from 'lucide-react';
import * as api from '../lib/api';
import { Backend } from '../types';

interface Props {
  systemInfo: api.SystemInfo | null;
  wslStatus: api.WslStatus | null;
  depStatus: api.DependencyStatus | null;
  backend: Backend;
  setBackend: (b: Backend) => void;
  onDetectSystem: () => void;
  onInstallLlamaServer: () => void;
  installingLlamaServer: boolean;
  installProgress: number;
  onProceedToModels: () => void;
}

export default function SetupTab({
  systemInfo, wslStatus, depStatus, backend, setBackend,
  onDetectSystem, onInstallLlamaServer, installingLlamaServer, installProgress,
  onProceedToModels,
}: Props) {
  return (
    <div className="h-full p-6 overflow-auto">
      <div className="max-w-2xl mx-auto">
        <div className="flex items-center justify-between mb-6">
          <h2 className="text-xl font-semibold text-white">System Setup</h2>
          <button
            onClick={onDetectSystem}
            className="flex items-center gap-2 px-3 py-2 text-sm text-zinc-400 hover:text-white hover:bg-zinc-800 rounded-lg"
          >
            <RefreshCw className="w-4 h-4" />
            Refresh
          </button>
        </div>

        {/* System Hardware */}
        <div className="bg-zinc-800 rounded-xl p-6 mb-4">
          <h3 className="text-white font-medium mb-4 flex items-center gap-2">
            <Cpu className="w-5 h-5" />
            System Hardware
          </h3>
          <div className="space-y-3">
            {/* CPU */}
            {systemInfo?.cpu && (
              <div className="flex items-center justify-between p-3 bg-zinc-700/50 rounded-lg">
                <div>
                  <p className="text-white">{systemInfo.cpu.name}</p>
                  <p className="text-zinc-400 text-sm">
                    {systemInfo.cpu.cores} cores / {systemInfo.cpu.threads} threads
                  </p>
                </div>
                <div className="px-2 py-1 rounded text-xs font-medium bg-blue-500/20 text-blue-400">
                  CPU
                </div>
              </div>
            )}
            {/* RAM */}
            {systemInfo?.ram && (
              <div className="flex items-center justify-between p-3 bg-zinc-700/50 rounded-lg">
                <div>
                  <p className="text-white">{systemInfo.ram.total_gb.toFixed(1)} GB RAM</p>
                  <p className="text-zinc-400 text-sm">System Memory</p>
                </div>
                <div className="px-2 py-1 rounded text-xs font-medium bg-purple-500/20 text-purple-400">
                  RAM
                </div>
              </div>
            )}
          </div>
        </div>

        {/* GPU Detection */}
        <div className="bg-zinc-800 rounded-xl p-6 mb-4">
          <h3 className="text-white font-medium mb-4 flex items-center gap-2">
            <Monitor className="w-5 h-5" />
            Graphics Cards
          </h3>
          {systemInfo?.gpus.length === 0 ? (
            <p className="text-zinc-500">No GPUs detected</p>
          ) : (
            <div className="space-y-3">
              {systemInfo?.gpus.map((gpu) => (
                <div key={gpu.name} className="flex items-center justify-between p-3 bg-zinc-700/50 rounded-lg">
                  <div>
                    <p className="text-white">{gpu.name}</p>
                    <p className="text-zinc-400 text-sm">
                      {gpu.vendor} - {api.formatVram(gpu.vram_mb)} VRAM
                    </p>
                  </div>
                  <div className={`px-2 py-1 rounded text-xs font-medium ${
                    gpu.vendor === 'NVIDIA' ? 'bg-green-500/20 text-green-400' :
                    gpu.vendor === 'AMD' ? 'bg-red-500/20 text-red-400' :
                    'bg-zinc-500/20 text-zinc-400'
                  }`}>
                    {gpu.vendor}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* WSL Status */}
        <div className="bg-zinc-800 rounded-xl p-6 mb-4">
          <h3 className="text-white font-medium mb-4 flex items-center gap-2">
            <Cpu className="w-5 h-5" />
            WSL2 Status
          </h3>
          {wslStatus?.installed ? (
            <div className="space-y-3">
              <div className="flex items-center gap-2">
                <Check className="w-5 h-5 text-green-400" />
                <span className="text-green-400">WSL2 Installed</span>
              </div>
              {wslStatus.distro && (
                <p className="text-zinc-400">Distro: <span className="text-white">{wslStatus.distro}</span></p>
              )}
              {wslStatus.llama_server_path ? (
                <div className="flex items-center gap-2">
                  <Check className="w-5 h-5 text-green-400" />
                  <span className="text-zinc-400">llama-server: <span className="text-white">{wslStatus.llama_server_path}</span></span>
                </div>
              ) : (
                <div className="flex items-center gap-2">
                  <X className="w-5 h-5 text-amber-400" />
                  <span className="text-amber-400">llama-server not found in WSL</span>
                </div>
              )}
              {wslStatus.rocm_version && (
                <p className="text-zinc-400">ROCm: <span className="text-white">{wslStatus.rocm_version}</span></p>
              )}
              {wslStatus.cuda_version && (
                <p className="text-zinc-400">CUDA: <span className="text-white">{wslStatus.cuda_version}</span></p>
              )}
            </div>
          ) : (
            <div className="flex items-center gap-2">
              <X className="w-5 h-5 text-zinc-500" />
              <span className="text-zinc-500">WSL2 not installed</span>
            </div>
          )}
        </div>

        {/* Runtime Dependencies */}
        <div className="bg-zinc-800 rounded-xl p-6 mb-4">
          <h3 className="text-white font-medium mb-4 flex items-center gap-2">
            <Download className="w-5 h-5" />
            Runtime Dependencies
          </h3>

          {depStatus?.ready_to_run ? (
            <div className="flex items-center gap-2 p-3 bg-green-500/10 border border-green-500/30 rounded-lg">
              <Check className="w-5 h-5 text-green-400" />
              <span className="text-green-400">All dependencies satisfied - ready to run!</span>
            </div>
          ) : (
            <div className="space-y-3">
              {/* Missing deps list */}
              {depStatus?.missing_deps.map((dep) => (
                <div key={dep} className="flex items-center gap-2 p-3 bg-red-500/10 border border-red-500/30 rounded-lg">
                  <X className="w-5 h-5 text-red-400" />
                  <span className="text-red-400">{dep}</span>
                </div>
              ))}

              {/* Windows llama-server install button */}
              {depStatus?.recommended_backend === 'windows' && !depStatus?.windows_llama_server && (
                <div className="mt-4 p-4 bg-zinc-700/50 rounded-lg">
                  <p className="text-white font-medium mb-2">Install llama-server for Windows</p>
                  <p className="text-zinc-400 text-sm mb-3">
                    Download the pre-built llama-server from llama.cpp releases (includes CUDA support for NVIDIA GPUs).
                  </p>
                  <button
                    onClick={onInstallLlamaServer}
                    disabled={installingLlamaServer}
                    className="w-full py-2 bg-amber-500 hover:bg-amber-600 disabled:opacity-50 text-black font-medium rounded-lg flex items-center justify-center gap-2"
                  >
                    {installingLlamaServer ? (
                      <>
                        <Loader2 className="w-4 h-4 animate-spin" />
                        Downloading... {installProgress}%
                      </>
                    ) : (
                      <>
                        <Download className="w-4 h-4" />
                        Download llama-server
                      </>
                    )}
                  </button>
                </div>
              )}

              {/* WSL/AMD setup guide */}
              {depStatus?.recommended_backend === 'wsl' && (
                <div className="mt-4 p-4 bg-zinc-700/50 rounded-lg">
                  <p className="text-white font-medium mb-2">Setup Guide for AMD GPUs</p>

                  {!depStatus?.wsl_installed && (
                    <div className="mb-3">
                      <p className="text-amber-400 text-sm font-medium">1. Install WSL2</p>
                      <p className="text-zinc-500 text-xs font-mono mt-1">
                        wsl --install -d Ubuntu
                      </p>
                    </div>
                  )}

                  {depStatus?.wsl_installed && !depStatus?.rocm_available && (
                    <div className="mb-3">
                      <p className="text-amber-400 text-sm font-medium">
                        {depStatus?.wsl_installed ? '1' : '2'}. Install ROCm in WSL
                      </p>
                      <p className="text-zinc-400 text-xs mt-1">
                        Follow the{' '}
                        <a
                          href="https://rocm.docs.amd.com/projects/install-on-linux/en/latest/tutorial/quick-start.html"
                          target="_blank"
                          rel="noopener noreferrer"
                          className="text-amber-400 underline"
                        >
                          AMD ROCm installation guide
                        </a>
                      </p>
                    </div>
                  )}

                  {depStatus?.wsl_installed && !depStatus?.wsl_llama_server && (
                    <div className="mb-3">
                      <p className="text-amber-400 text-sm font-medium">
                        {!depStatus?.rocm_available ? '3' : '2'}. Build llama.cpp with ROCm
                      </p>
                      <p className="text-zinc-500 text-xs font-mono mt-1">
                        git clone https://github.com/ggerganov/llama.cpp<br />
                        cd llama.cpp<br />
                        make GGML_HIPBLAS=1 -j$(nproc)
                      </p>
                    </div>
                  )}

                  <button
                    onClick={onDetectSystem}
                    className="w-full py-2 mt-2 bg-zinc-600 hover:bg-zinc-500 text-white font-medium rounded-lg flex items-center justify-center gap-2"
                  >
                    <RefreshCw className="w-4 h-4" />
                    Check Again
                  </button>
                </div>
              )}
            </div>
          )}
        </div>

        {/* Backend Selection */}
        <div className="bg-zinc-800 rounded-xl p-6 mb-6">
          <h3 className="text-white font-medium mb-4 flex items-center gap-2">
            <Settings className="w-5 h-5" />
            Backend Selection
          </h3>
          <div className="flex gap-3">
            <button
              onClick={() => setBackend('windows')}
              className={`flex-1 p-4 rounded-lg border-2 transition-all ${
                backend === 'windows'
                  ? 'border-amber-500 bg-amber-500/10'
                  : 'border-zinc-600 hover:border-zinc-500'
              }`}
            >
              <p className="text-white font-medium">Windows Native</p>
              <p className="text-zinc-400 text-sm mt-1">For NVIDIA GPUs with CUDA</p>
              {depStatus?.windows_llama_server && (
                <p className="text-green-400 text-xs mt-1 flex items-center gap-1">
                  <Check className="w-3 h-3" /> llama-server ready
                </p>
              )}
            </button>
            <button
              onClick={() => setBackend('wsl')}
              disabled={!wslStatus?.installed}
              className={`flex-1 p-4 rounded-lg border-2 transition-all ${
                backend === 'wsl'
                  ? 'border-amber-500 bg-amber-500/10'
                  : 'border-zinc-600 hover:border-zinc-500'
              } disabled:opacity-50 disabled:cursor-not-allowed`}
            >
              <p className="text-white font-medium">WSL2 (Linux)</p>
              <p className="text-zinc-400 text-sm mt-1">For AMD GPUs with ROCm</p>
              {depStatus?.wsl_llama_server && (
                <p className="text-green-400 text-xs mt-1 flex items-center gap-1">
                  <Check className="w-3 h-3" /> llama-server ready
                </p>
              )}
            </button>
          </div>
          {depStatus?.recommended_backend && (
            <p className="text-zinc-400 text-sm mt-4">
              Recommended: <span className="text-amber-400">{depStatus.recommended_backend.toUpperCase()}</span>
              {systemInfo?.gpus.some(g => g.vendor === 'AMD') && ' (AMD GPU detected)'}
              {systemInfo?.gpus.some(g => g.vendor === 'NVIDIA') && ' (NVIDIA GPU detected)'}
            </p>
          )}
        </div>

        {/* Continue Button */}
        <button
          onClick={onProceedToModels}
          disabled={!depStatus?.ready_to_run && !(
            (backend === 'windows' && depStatus?.windows_llama_server) ||
            (backend === 'wsl' && depStatus?.wsl_llama_server)
          )}
          className="w-full py-3 bg-amber-500 hover:bg-amber-600 disabled:opacity-50 disabled:cursor-not-allowed text-black font-medium rounded-xl"
        >
          {depStatus?.ready_to_run || (backend === 'windows' && depStatus?.windows_llama_server) || (backend === 'wsl' && depStatus?.wsl_llama_server)
            ? 'Continue to Models'
            : 'Install Dependencies First'}
        </button>
      </div>
    </div>
  );
}
