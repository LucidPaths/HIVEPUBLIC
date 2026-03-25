import { useState } from 'react';
import * as api from '../lib/api';

interface UseHuggingFaceProps {
  systemInfo: api.SystemInfo | null;
  onError: (error: string) => void;
  onModelsLoaded?: () => void;
}

export function useHuggingFace({ systemInfo, onError, onModelsLoaded }: UseHuggingFaceProps) {
  const [hfSearch, setHfSearch] = useState('');
  const [hfModels, setHfModels] = useState<api.HfModel[]>([]);
  const [hfLoading, setHfLoading] = useState(false);
  const [selectedHfModel, setSelectedHfModel] = useState<api.HfModel | null>(null);
  const [hfFiles, setHfFiles] = useState<api.HfModelFile[]>([]);
  const [downloading, setDownloading] = useState<string | null>(null);
  const [downloadProgress, setDownloadProgress] = useState(0);

  // VRAM compatibility state
  const [vramCompatibility, setVramCompatibility] = useState<Record<string, api.VramCompatibility>>({});

  // Recommended models (computed from real HuggingFace data)
  const [recommendedModels, setRecommendedModels] = useState<api.RecommendedModel[]>([]);
  const [recsLoading, setRecsLoading] = useState(false);

  async function searchHuggingFace(queryOverride?: string) {
    const query = queryOverride ?? hfSearch;
    console.log('[HIVE] searchHuggingFace: Searching for:', query);
    setHfLoading(true);
    try {
      const results = await api.searchHfModels(query);
      console.log('[HIVE] searchHuggingFace: Found', results.length, 'models');
      setHfModels(results);

      // Compute recommendations in background (only on initial/empty search)
      if (!query) {
        computeRecommendations(results);
      }
    } catch (e) {
      console.error('[HIVE] searchHuggingFace: Error:', e);
      onError(String(e));
    } finally {
      setHfLoading(false);
    }
  }

  async function computeRecommendations(models: api.HfModel[]) {
    const primaryGpu = systemInfo?.gpus?.[0];
    if (!primaryGpu || primaryGpu.vram_mb <= 0) return;

    setRecsLoading(true);
    console.log('[HIVE] recommendations: Computing for', models.length, 'models...');
    const ramGb = systemInfo?.ram?.total_gb ?? 0;
    // Conservative RAM budget: 50% of total (rest for OS + other apps)
    const conservativeRamGb = ramGb * 0.5;

    try {
      // Phase 1: Fetch files + benchmark scores in parallel
      const baseModelIds = models.filter(m => m.baseModel).map(m => m.baseModel!);

      const [fileResults, benchmarkScores] = await Promise.all([
        // Fetch file lists for all models
        Promise.all(
          models.map(async (model) => {
            try {
              const files = await api.getHfModelFiles(model.id);
              return { modelId: model.id, files };
            } catch {
              return { modelId: model.id, files: [] as api.HfModelFile[] };
            }
          })
        ),
        // Fetch benchmark scores for known base models
        baseModelIds.length > 0
          ? api.fetchBenchmarkScores(baseModelIds)
          : Promise.resolve({} as Record<string, number>),
      ]);

      // Attach benchmark scores to models
      for (const model of models) {
        if (model.baseModel && benchmarkScores[model.baseModel] !== undefined) {
          model.qualityScore = benchmarkScores[model.baseModel];
        }
      }
      console.log('[HIVE] recommendations: Got benchmark scores for', Object.keys(benchmarkScores).length, 'base models');

      const filesByModel: Record<string, api.HfModelFile[]> = {};
      for (const r of fileResults) {
        filesByModel[r.modelId] = r.files;
      }

      // Phase 2: Compute VRAM compatibility for all files
      const compatByModel: Record<string, Record<string, api.VramCompatibility>> = {};
      await Promise.all(
        models.map(async (model) => {
          const files = filesByModel[model.id] || [];
          const compat: Record<string, api.VramCompatibility> = {};
          const results = await Promise.all(
            files.map(async (file) => {
              try {
                const c = await api.checkVramCompatibility(
                  file.size, file.filename, primaryGpu.vram_mb, 4096
                );
                return { filename: file.filename, compat: c };
              } catch {
                return null;
              }
            })
          );
          for (const r of results) {
            if (r) compat[r.filename] = r.compat;
          }
          compatByModel[model.id] = compat;
        })
      );

      // Build recommendations with conservative RAM budget
      const recs = api.buildRecommendations(models, filesByModel, compatByModel, conservativeRamGb);
      console.log('[HIVE] recommendations: Found', recs.length, 'compatible models');
      setRecommendedModels(recs);
    } catch (e) {
      console.error('[HIVE] recommendations: Error:', e);
    } finally {
      setRecsLoading(false);
    }
  }

  async function selectHfModel(model: api.HfModel) {
    console.log('[HIVE] selectHfModel: Selected', model.id);
    setSelectedHfModel(model);
    setVramCompatibility({});
    try {
      const files = await api.getHfModelFiles(model.id);
      setHfFiles(files);

      const primaryGpu = systemInfo?.gpus?.[0];
      if (primaryGpu && primaryGpu.vram_mb > 0) {
        const compatibilityPromises = files.map(async (file) => {
          try {
            const compat = await api.checkVramCompatibility(
              file.size,
              file.filename,
              primaryGpu.vram_mb,
              4096
            );
            return { filename: file.filename, compat };
          } catch {
            return null;
          }
        });

        const results = await Promise.all(compatibilityPromises);
        const compatMap: Record<string, api.VramCompatibility> = {};
        for (const result of results) {
          if (result) {
            compatMap[result.filename] = result.compat;
          }
        }
        setVramCompatibility(compatMap);
      }
    } catch (e) {
      onError(String(e));
      setHfFiles([]);
    }
  }

  async function downloadFile(file: api.HfModelFile) {
    console.log('[HIVE] downloadFile: Starting download:', file.filename, `(${(file.size / 1024 / 1024 / 1024).toFixed(2)} GB)`);
    setDownloading(file.filename);
    setDownloadProgress(0);
    try {
      console.log('[HIVE] downloadFile: Using native download path');
      await api.downloadModel(file.downloadUrl, file.filename, (downloaded, total) => {
        setDownloadProgress(Math.round((downloaded / total) * 100));
      });
      console.log('[HIVE] downloadFile: Download complete, refreshing model list');
      onModelsLoaded?.();
    } catch (e) {
      console.error('[HIVE] downloadFile: Error:', e);
      onError(String(e));
    } finally {
      setDownloading(null);
      setDownloadProgress(0);
    }
  }

  return {
    hfSearch, setHfSearch,
    hfModels,
    hfLoading,
    selectedHfModel,
    hfFiles,
    downloading,
    downloadProgress,
    vramCompatibility,
    recommendedModels,
    recsLoading,
    searchHuggingFace,
    selectHfModel,
    downloadFile,
  };
}
