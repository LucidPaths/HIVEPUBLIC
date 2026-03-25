import { useMemo } from 'react';
import { Search, Loader2, ArrowDownToLine, Heart, Download, Monitor } from 'lucide-react';
import * as api from '../lib/api';

interface Props {
  hfSearch: string;
  setHfSearch: (s: string) => void;
  hfModels: api.HfModel[];
  hfLoading: boolean;
  selectedHfModel: api.HfModel | null;
  hfFiles: api.HfModelFile[];
  downloading: string | null;
  downloadProgress: number;
  vramCompatibility: Record<string, api.VramCompatibility>;
  systemInfo: api.SystemInfo | null;
  recommendedModels: api.RecommendedModel[];
  recsLoading: boolean;
  onSearchHuggingFace: (queryOverride?: string) => void;
  onSelectHfModel: (model: api.HfModel) => void;
  onDownloadFile: (file: api.HfModelFile) => void;
}

export default function BrowseTab({
  hfSearch, setHfSearch, hfModels, hfLoading,
  selectedHfModel, hfFiles, downloading, downloadProgress,
  vramCompatibility, systemInfo, recommendedModels, recsLoading,
  onSearchHuggingFace, onSelectHfModel, onDownloadFile,
}: Props) {
  const primaryGpu = systemInfo?.gpus?.[0];
  const ramGb = systemInfo?.ram?.total_gb ?? 0;

  // Sort files: fast first, then good, slow, too_large
  const sortedFiles = useMemo(
    () => api.sortFilesByCompatibility(hfFiles, vramCompatibility, ramGb),
    [hfFiles, vramCompatibility, ramGb]
  );

  // Group recommendations by category (GPU utilization bands)
  const fastRecs = useMemo(() => recommendedModels.filter(r => r.category === 'fast'), [recommendedModels]);
  const qualityRecs = useMemo(() => recommendedModels.filter(r => r.category === 'quality'), [recommendedModels]);
  const brainRecs = useMemo(() => recommendedModels.filter(r => r.category === 'brain'), [recommendedModels]);

  // Category-specific colors (consistent regardless of speedTier from Rust)
  const catColors: Record<api.RecCategory, { bg: string; border: string }> = {
    fast:    { bg: 'bg-green-500/20',  border: 'border-green-500/50' },
    quality: { bg: 'bg-yellow-500/20', border: 'border-yellow-500/50' },
    brain:   { bg: 'bg-blue-500/20',   border: 'border-blue-500/50' },
  };
  const catIcons: Record<api.RecCategory, string> = {
    fast: '\u26A1',       // ⚡
    quality: '\u2705',    // ✅
    brain: '\uD83E\uDDE0', // 🧠
  };

  function renderRecCard(rec: api.RecommendedModel) {
    const isSelected = selectedHfModel?.id === rec.model.id;
    const color = catColors[rec.category];
    const scoreText = rec.qualityScore != null ? ` | Benchmark: ${rec.qualityScore.toFixed(1)}%` : '';
    return (
      <button
        key={`${rec.model.id}-${rec.category}`}
        onClick={() => onSelectHfModel(rec.model)}
        title={`${rec.speedTier.detail}${scoreText}\nFile: ${rec.bestFile.filename} (${api.formatBytes(rec.bestFile.size)})`}
        className={`w-full text-left p-2 rounded-lg border transition-colors text-xs ${
          isSelected
            ? 'bg-amber-500/20 border-amber-500'
            : `${color.bg} ${color.border} hover:brightness-125`
        }`}
      >
        <div className="flex items-center gap-1.5">
          <span>{catIcons[rec.category]}</span>
          <span className="text-white font-medium truncate">{rec.model.name}</span>
        </div>
        <div className="flex items-center gap-2 mt-0.5 text-zinc-400">
          <span>{api.formatBytes(rec.bestFile.size)}</span>
          <span className="text-zinc-600">·</span>
          <span>{rec.model.downloads.toLocaleString()} dl</span>
        </div>
      </button>
    );
  }

  const hasRecs = recommendedModels.length > 0 || recsLoading;

  return (
    <div className="h-full flex">
      {/* Left panel */}
      <div className="w-80 border-r border-zinc-700 flex flex-col">
        {/* Sticky search bar */}
        <div className="p-4 pb-2 shrink-0">
          <div className="flex gap-2 mb-2">
            <input
              type="text"
              value={hfSearch}
              onChange={(e) => setHfSearch(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && onSearchHuggingFace()}
              placeholder="Search GGUF models..."
              className="flex-1 bg-zinc-800 text-white px-3 py-2 rounded-lg border border-zinc-600 text-sm"
            />
            <button
              onClick={() => onSearchHuggingFace()}
              disabled={hfLoading}
              className="p-2 bg-amber-500 hover:bg-amber-600 text-black rounded-lg"
            >
              {hfLoading ? <Loader2 className="w-5 h-5 animate-spin" /> : <Search className="w-5 h-5" />}
            </button>
          </div>

          {/* Hardware summary */}
          {primaryGpu && (
            <div className="flex items-center gap-1.5">
              <Monitor className="w-3.5 h-3.5 text-amber-400" />
              <span className="text-xs text-zinc-400">
                {primaryGpu.name} ({api.formatVram(primaryGpu.vram_mb)})
                {ramGb > 0 && <span className="text-zinc-600"> + {ramGb.toFixed(0)} GB RAM</span>}
              </span>
            </div>
          )}
        </div>

        {/* Scrollable: recommendations + all results */}
        <div className="flex-1 overflow-auto px-4 pb-4">
          {/* Recommended models grouped by speed tier */}
          {hasRecs && (
            <div className="mb-3 pt-1">
              <div className="flex items-center gap-1.5 mb-2">
                <span className="text-xs font-medium text-zinc-300">Recommended for you</span>
                {recsLoading && <Loader2 className="w-3 h-3 animate-spin text-zinc-500" />}
              </div>

              {/* Fast picks — fits entirely in GPU */}
              {fastRecs.length > 0 && (
                <div className="mb-2">
                  <div className="text-[10px] uppercase tracking-wider text-green-500 mb-1">
                    {'\u26A1'} Fast picks
                  </div>
                  <div className="space-y-1">
                    {fastRecs.slice(0, 2).map(renderRecCard)}
                  </div>
                </div>
              )}

              {/* Quality picks — tight GPU fit */}
              {qualityRecs.length > 0 && (
                <div className="mb-2">
                  <div className="text-[10px] uppercase tracking-wider text-yellow-500 mb-1">
                    {'\u2705'} Best quality
                  </div>
                  <div className="space-y-1">
                    {qualityRecs.slice(0, 2).map(renderRecCard)}
                  </div>
                </div>
              )}

              {/* RAM offload picks — bigger, slower but smarter */}
              {brainRecs.length > 0 && (
                <div className="mb-2">
                  <div className="text-[10px] uppercase tracking-wider text-blue-500 mb-1">
                    {'\uD83E\uDDE0'} Big brain (uses RAM)
                  </div>
                  <div className="space-y-1">
                    {brainRecs.slice(0, 2).map(renderRecCard)}
                  </div>
                </div>
              )}
            </div>
          )}

          {/* All results */}
          {hfModels.length > 0 && (
            <div className="text-xs text-zinc-500 mb-2">All results</div>
          )}
          <div className="space-y-2">
            {hfModels.map((model) => (
              <div
                key={model.id}
                onClick={() => onSelectHfModel(model)}
                className={`p-3 rounded-lg cursor-pointer transition-colors ${
                  selectedHfModel?.id === model.id
                    ? 'bg-amber-500/20 border border-amber-500'
                    : 'bg-zinc-800 hover:bg-zinc-700 border border-transparent'
                }`}
              >
                <p className="text-white text-sm font-medium truncate">{model.name}</p>
                <p className="text-zinc-500 text-xs truncate">{model.author}</p>
                <div className="flex items-center gap-3 mt-1 text-xs text-zinc-400">
                  <span className="flex items-center gap-1">
                    <ArrowDownToLine className="w-3 h-3" />
                    {model.downloads.toLocaleString()}
                  </span>
                  <span className="flex items-center gap-1">
                    <Heart className="w-3 h-3" />
                    {model.likes}
                  </span>
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>

      {/* Files panel */}
      <div className="flex-1 p-6 overflow-auto">
        {selectedHfModel ? (
          <div>
            <h2 className="text-xl font-semibold text-white mb-1">{selectedHfModel.name}</h2>
            <p className="text-zinc-400 text-sm">by {selectedHfModel.author}</p>

            {/* Model description from HuggingFace metadata */}
            <div className="mt-2 mb-4 flex flex-wrap items-center gap-1.5">
              {selectedHfModel.pipelineTag && (
                <span className="text-xs text-amber-300 bg-amber-500/15 px-2 py-0.5 rounded border border-amber-500/30">
                  {api.getPipelineLabel(selectedHfModel.pipelineTag)}
                </span>
              )}
              {selectedHfModel.domainTags?.map(tag => (
                <span key={tag} className="text-xs text-zinc-300 bg-zinc-700/60 px-2 py-0.5 rounded border border-zinc-600/50">
                  {tag}
                </span>
              ))}
              {!selectedHfModel.pipelineTag && !selectedHfModel.domainTags?.length && (
                <span className="text-xs text-zinc-500">Language Model</span>
              )}
            </div>

            <div className="flex items-center justify-between mb-3">
              <h3 className="text-white font-medium">GGUF Files</h3>
              {primaryGpu && (
                <div className="text-xs text-zinc-400 flex items-center gap-2">
                  <Monitor className="w-3 h-3" />
                  {api.formatVram(primaryGpu.vram_mb)} GPU
                  {ramGb > 0 && <span>+ {ramGb.toFixed(0)} GB RAM</span>}
                </div>
              )}
            </div>

            {/* Legend — noob-friendly */}
            {primaryGpu && sortedFiles.length > 0 && (
              <div className="mb-3 p-2 bg-zinc-700/30 rounded-lg text-xs text-zinc-400 flex flex-wrap items-center gap-x-3 gap-y-1">
                <span className="text-zinc-500">Speed:</span>
                <span>{'\u26A1'} Fast</span>
                <span>{'\u2705'} Runs well</span>
                <span>{'\uD83D\uDC22'} Slower (uses RAM)</span>
                <span>{'\u274C'} Too large</span>
              </div>
            )}

            {sortedFiles.length === 0 ? (
              <p className="text-zinc-500">No GGUF files found in this repo.</p>
            ) : (
              <div className="space-y-2">
                {sortedFiles.map((file) => {
                  const compat = vramCompatibility[file.filename];
                  const speed = compat ? api.getSpeedTier(compat, ramGb) : null;

                  return (
                    <div
                      key={file.filename}
                      className={`flex items-center justify-between p-3 bg-zinc-800 rounded-lg ${
                        speed ? `border ${speed.color.border}` : ''
                      }`}
                    >
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2">
                          {speed && (
                            <span className="text-sm cursor-default" title={speed.detail}>
                              {speed.icon}
                            </span>
                          )}
                          <p className="text-white text-sm truncate">{file.filename}</p>
                        </div>
                        <div className="flex items-center gap-3 mt-1">
                          <p className="text-zinc-500 text-xs">{api.formatBytes(file.size)}</p>
                          {speed && (
                            <span
                              className={`text-xs px-1.5 py-0.5 rounded cursor-default ${speed.color.bg} ${speed.color.text}`}
                              title={speed.detail}
                            >
                              {speed.label}
                            </span>
                          )}
                          {compat && speed && speed.tier !== 'too_large' && (
                            <span className="text-xs text-zinc-600">
                              ~{compat.estimate.total_gb.toFixed(1)} GB
                            </span>
                          )}
                        </div>
                      </div>
                      <button
                        onClick={() => onDownloadFile(file)}
                        disabled={downloading === file.filename || speed?.tier === 'too_large'}
                        className={`px-3 py-1.5 text-sm rounded-lg flex items-center gap-2 ml-3 ${
                          speed?.tier === 'too_large'
                            ? 'bg-zinc-700 text-zinc-500 cursor-not-allowed'
                            : 'bg-amber-500 hover:bg-amber-600 disabled:opacity-50 text-black'
                        }`}
                      >
                        {downloading === file.filename ? (
                          <>
                            <Loader2 className="w-4 h-4 animate-spin" />
                            {downloadProgress}%
                          </>
                        ) : (
                          <>
                            <Download className="w-4 h-4" />
                            Download
                          </>
                        )}
                      </button>
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        ) : (
          <div className="h-full flex flex-col items-center justify-center text-zinc-500">
            {primaryGpu ? (
              <>
                <Monitor className="w-8 h-8 mb-3 text-zinc-600" />
                <p className="mb-1">Select a model to see available files</p>
                <p className="text-xs text-zinc-600">
                  Files are rated by speed on your {api.formatVram(primaryGpu.vram_mb)} GPU
                  {ramGb > 0 && ` + ${ramGb.toFixed(0)} GB RAM`}
                </p>
              </>
            ) : (
              <p>Search and select a model to see available files</p>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
