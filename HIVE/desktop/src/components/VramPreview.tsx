import { useState, useEffect } from 'react';
import * as api from '../lib/api';

export default function VramPreview({ model, gpu, settings }: {
  model: api.LocalModel;
  gpu: api.GpuInfo;
  settings: api.ModelSettings;
}) {
  const [estimate, setEstimate] = useState<api.VramEstimate | null>(null);

  useEffect(() => {
    api.estimateModelVram(
      model.size_bytes,
      model.filename,
      model.path,
      settings.contextLength,
      !settings.kvOffload // includeKvCache is opposite of kvOffload
    ).then(setEstimate).catch(() => setEstimate(null));
  }, [model, settings.contextLength, settings.kvOffload]);

  if (!estimate) return <div className="text-zinc-500">Calculating...</div>;

  const availableGb = gpu.vram_mb / 1024;
  const headroom = availableGb - estimate.total_gb;
  const status = headroom > 2 ? 'good' : headroom > 0 ? 'tight' : 'insufficient';
  const statusIcon = api.getVramStatusIcon(status);
  const badgeColors = api.getVramBadgeColor(status);

  return (
    <div className="space-y-3">
      {/* Status Badge */}
      <div className={`flex items-center gap-2 p-3 rounded-lg ${badgeColors.bg} border ${badgeColors.border}`}>
        <span className="text-xl">{statusIcon}</span>
        <div>
          <p className={badgeColors.text + ' font-medium'}>
            {status === 'good' ? 'Will run comfortably' :
             status === 'tight' ? 'Will run at limit' :
             'May not fit in VRAM'}
          </p>
          <p className="text-zinc-400 text-sm">
            {estimate.total_gb.toFixed(1)} GB needed / {availableGb.toFixed(1)} GB available
            {headroom > 0 && ` (${headroom.toFixed(1)} GB headroom)`}
          </p>
        </div>
      </div>

      {/* Breakdown */}
      <div className="grid grid-cols-3 gap-2 text-sm">
        <div className="p-2 bg-zinc-700/50 rounded">
          <p className="text-zinc-400">Weights</p>
          <p className="text-white font-mono">{estimate.model_weights_gb.toFixed(2)} GB</p>
        </div>
        <div className={`p-2 rounded ${settings.kvOffload ? 'bg-zinc-700/30 opacity-50' : 'bg-zinc-700/50'}`}>
          <p className="text-zinc-400">KV Cache {settings.kvOffload && '(RAM)'}</p>
          <p className="text-white font-mono">{estimate.kv_cache_gb.toFixed(2)} GB</p>
        </div>
        <div className="p-2 bg-zinc-700/50 rounded">
          <p className="text-zinc-400">Overhead</p>
          <p className="text-white font-mono">{estimate.overhead_gb.toFixed(2)} GB</p>
        </div>
      </div>

      {/* Quantization Info */}
      <p className="text-zinc-500 text-xs">
        Quantization: {estimate.quantization} • Context: {estimate.context_length.toLocaleString()} tokens
        • Confidence: {estimate.confidence}
      </p>
    </div>
  );
}
