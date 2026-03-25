import { X, Settings, StopCircle, Play, Loader2 } from 'lucide-react';
import * as api from '../lib/api';
import { Tab } from '../types';
import VramPreview from './VramPreview';

interface Props {
  selectedModel: api.LocalModel;
  serverRunning: boolean;
  serverLoading: boolean;
  systemInfo: api.SystemInfo | null;
  modelSettings: api.ModelSettings;
  onClose: () => void;
  onStartModel: () => void;
  onStopModel: () => void;
  onSetTab: (tab: Tab) => void;
}

export default function ModelInfoPopup({
  selectedModel, serverRunning, serverLoading, systemInfo, modelSettings,
  onClose, onStartModel, onStopModel, onSetTab,
}: Props) {
  return (
    <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50" onClick={onClose}>
      <div
        className="bg-zinc-800 rounded-xl p-6 max-w-md w-full mx-4 border border-zinc-600 shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-white font-semibold text-lg">Model Info</h3>
          <button
            onClick={onClose}
            className="text-zinc-400 hover:text-white p-1 rounded hover:bg-zinc-700"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Model Details */}
        <div className="space-y-3 mb-6">
          <div className="p-3 bg-zinc-700/50 rounded-lg">
            <p className="text-white font-medium">{selectedModel.filename}</p>
            <p className="text-zinc-400 text-sm mt-1">
              Size: {selectedModel.size_gb.toFixed(2)} GB
            </p>
            {selectedModel.path.startsWith('/') && (
              <span className="inline-block mt-2 text-xs bg-zinc-600 px-2 py-0.5 rounded text-zinc-300">WSL</span>
            )}
          </div>

          {/* VRAM estimate if we have GPU info */}
          {systemInfo?.gpus?.[0] && (
            <div className="p-3 bg-zinc-700/50 rounded-lg">
              <p className="text-zinc-400 text-sm mb-2">VRAM Estimate</p>
              <VramPreview
                model={selectedModel}
                gpu={systemInfo.gpus[0]}
                settings={modelSettings}
              />
            </div>
          )}

          {/* System Prompt Preview */}
          {modelSettings.systemPrompt && (
            <div className="p-3 bg-zinc-700/50 rounded-lg">
              <p className="text-zinc-400 text-sm mb-1">System Prompt</p>
              <p className="text-white text-sm truncate">{modelSettings.systemPrompt.substring(0, 100)}...</p>
            </div>
          )}
        </div>

        {/* Actions */}
        <div className="flex gap-3">
          <button
            onClick={() => {
              onClose();
              onSetTab('settings');
            }}
            className="flex-1 py-2.5 bg-zinc-700 hover:bg-zinc-600 text-white font-medium rounded-lg flex items-center justify-center gap-2"
          >
            <Settings className="w-4 h-4" />
            Configure
          </button>
          {serverRunning ? (
            <button
              onClick={() => {
                onClose();
                onStopModel();
              }}
              className="flex-1 py-2.5 bg-red-500 hover:bg-red-600 text-white font-medium rounded-lg flex items-center justify-center gap-2"
            >
              <StopCircle className="w-4 h-4" />
              Stop
            </button>
          ) : (
            <button
              onClick={() => {
                onClose();
                onStartModel();
              }}
              disabled={serverLoading}
              className="flex-1 py-2.5 bg-amber-500 hover:bg-amber-600 disabled:opacity-50 text-black font-medium rounded-lg flex items-center justify-center gap-2"
            >
              {serverLoading ? (
                <Loader2 className="w-4 h-4 animate-spin" />
              ) : (
                <Play className="w-4 h-4" />
              )}
              Load
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
