import { useState, useEffect } from 'react';
import { BookOpen, FolderOpen, RefreshCw } from 'lucide-react';
import * as api from '../../lib/api';

export default function SkillsSection() {
  const [skills, setSkills] = useState<api.SkillInfo[]>([]);
  const [skillsPath, setSkillsPath] = useState<string>('');
  const [loading, setLoading] = useState(false);

  async function loadSkills() {
    setLoading(true);
    try {
      const [list, path] = await Promise.all([
        api.harnessListSkills(),
        api.harnessGetSkillsPath(),
      ]);
      setSkills(list);
      setSkillsPath(path);
    } catch {
      // Non-fatal
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => { loadSkills(); }, []);

  return (
    <div className="bg-zinc-800 rounded-xl p-6 mt-6">
      <div className="flex items-center justify-between mb-4">
        <h3 className="text-white font-medium flex items-center gap-2">
          <BookOpen className="w-5 h-5" />
          Skills
        </h3>
        <div className="flex items-center gap-2">
          <button
            onClick={loadSkills}
            disabled={loading}
            className="flex items-center gap-1.5 px-3 py-1.5 text-sm bg-zinc-700 hover:bg-zinc-600 text-zinc-300 rounded-lg"
            title="Refresh skill list"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${loading ? 'animate-spin' : ''}`} />
            Refresh
          </button>
          {skillsPath && (
            <button
              onClick={() => {
                api.harnessOpenSkillsDir().catch(() => {});
              }}
              className="flex items-center gap-1.5 px-3 py-1.5 text-sm bg-amber-500 hover:bg-amber-600 text-black rounded-lg"
              title={`Open ${skillsPath}`}
            >
              <FolderOpen className="w-3.5 h-3.5" />
              Open Folder
            </button>
          )}
        </div>
      </div>

      <p className="text-zinc-400 text-sm mb-3">
        Skills are markdown files in <code className="text-amber-400">{skillsPath || '~/.hive/skills/'}</code>.
        Drop a <code className="text-amber-400">.md</code> file there to teach HIVE new capabilities.
        Relevant skills are auto-injected based on conversation context.
      </p>

      {skills.length === 0 ? (
        <p className="text-zinc-500 text-sm py-3 text-center">
          {loading ? 'Loading...' : 'No skill files found. Built-in seeds will be created on first use.'}
        </p>
      ) : (
        <div className="space-y-1.5">
          {skills.map(skill => (
            <div key={skill.name} className="flex items-center justify-between px-3 py-2 bg-zinc-900 rounded-lg">
              <div className="flex items-center gap-2">
                <BookOpen className="w-4 h-4 text-amber-400" />
                <span className="text-white text-sm">{skill.name}</span>
              </div>
              <span className="text-zinc-500 text-xs">
                {skill.size_bytes >= 1024
                  ? `${(skill.size_bytes / 1024).toFixed(1)} KB`
                  : `${skill.size_bytes} B`}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
