import React, { useEffect, useState } from 'react';
import {
  FileCode2,
  Download,
  Copy,
  Check,
  Terminal,
  Cpu,
  Layers,
} from 'lucide-react';
import {
  WorkspaceFileEntry,
  fetchWorkspaceFileContent,
  fetchWorkspaceManifest,
  isTauriNativeRuntime,
} from '../services/tauriBridge';
import type { EngineTuning, LinuxWaylandStrategy } from '../utils/kvmEngine';

interface WorkspaceSourceExplorerProps {
  tuning: EngineTuning;
  activeDriverName: string;
  onChangeWaylandStrategy: (strategy: LinuxWaylandStrategy) => void;
}

export const WorkspaceSourceExplorer: React.FC<WorkspaceSourceExplorerProps> = ({
  tuning,
  activeDriverName,
  onChangeWaylandStrategy,
}) => {
  const [files, setFiles] = useState<WorkspaceFileEntry[]>([]);
  const [selectedPath, setSelectedPath] = useState<string>(
    '.github/workflows/build.yml'
  );
  const [fileContent, setFileContent] = useState<string>('');
  const [fileSummary, setFileSummary] = useState<string>('');
  const [loadingFile, setLoadingFile] = useState<boolean>(true);
  const [copied, setCopied] = useState<boolean>(false);

  useEffect(() => {
    let active = true;
    fetchWorkspaceManifest().then((list) => {
      if (!active) return;
      setFiles(list);
    });
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    let active = true;
    setLoadingFile(true);
    fetchWorkspaceFileContent(selectedPath).then((detail) => {
      if (!active) return;
      if (detail) {
        setFileContent(detail.content);
        setFileSummary(detail.summary);
      }
      setLoadingFile(false);
    });
    return () => {
      active = false;
    };
  }, [selectedPath]);

  const handleCopySource = async () => {
    if (!fileContent) return;
    await navigator.clipboard.writeText(fileContent);
    setCopied(true);
    setTimeout(() => setCopied(false), 1800);
  };

  const totalRustLines = files.reduce((acc, f) => acc + f.lineCount, 0);

  return (
    <div className="space-y-6">
      {/* Header & Bundle Export Action */}
      <div className="flex flex-col lg:flex-row lg:items-center lg:justify-between gap-4 pb-5 border-b border-slate-200 dark:border-slate-800">
        <div>
          <h1 className="text-xl font-semibold tracking-tight text-slate-900 dark:text-slate-100">
            Rust + Tauri v2 跨平台工程源码与驱动配置
          </h1>
          <p className="text-sm text-slate-600 dark:text-slate-400 mt-1">
            已按您的确认完成 Tauri v2 桌面宿主（确认项 A）与 Linux Wayland{' '}
            <span className="font-mono">libei</span> +{' '}
            <span className="font-mono">/dev/uinput</span> 全兼容双栈驱动（确认项 B）工程落地。
          </p>
        </div>

        {isTauriNativeRuntime() ? (
          <button
            type="button"
            onClick={handleCopySource}
            className="inline-flex items-center gap-1.5 px-4 py-2 text-xs font-medium rounded-lg bg-blue-600 text-white hover:bg-blue-700 transition-colors whitespace-nowrap shrink-0"
          >
            {copied ? <Check className="w-3.5 h-3.5" /> : <Copy className="w-3.5 h-3.5" />}
            <span>{copied ? '已复制当前模块源码' : '复制当前模块源码'}</span>
          </button>
        ) : (
          <a
            href="/api/workspace/bundle.tar.gz"
            download="spankvm-rust-tauri-v2-workspace.tar.gz"
            className="inline-flex items-center gap-1.5 px-4 py-2 text-xs font-medium rounded-lg bg-blue-600 text-white hover:bg-blue-700 transition-colors whitespace-nowrap shrink-0"
          >
            <Download className="w-3.5 h-3.5" />
            打包下载完整工程 (.tar.gz)
          </a>
        )}
      </div>

      {/* Confirmed Option A & Option B Runtime Configuration Strip */}
      <section
        aria-label="已落地跨平台架构与 Wayland 驱动策略"
        className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl divide-y lg:divide-y-0 lg:divide-x divide-slate-200 dark:divide-slate-800 grid grid-cols-1 lg:grid-cols-3"
      >
        <div className="p-5 space-y-1">
          <div className="flex items-center gap-2 text-xs text-slate-500 dark:text-slate-400">
            <Layers className="w-3.5 h-3.5 text-blue-600 dark:text-blue-400 shrink-0" />
            <span>桌面宿主框架（确认项 A）</span>
          </div>
          <div className="text-sm font-semibold text-slate-900 dark:text-slate-100">
            Tauri v2 原生宿主 + 后台托盘守护
          </div>
          <div className="text-xs text-slate-500 dark:text-slate-400 tabular-nums">
            共 {files.length} 个核心工程文件 · {totalRustLines} 行跨端代码
          </div>
        </div>

        <div className="p-5 space-y-2">
          <div className="flex items-center gap-2 text-xs text-slate-500 dark:text-slate-400">
            <Cpu className="w-3.5 h-3.5 text-blue-600 dark:text-blue-400 shrink-0" />
            <label htmlFor="wayland-strategy-select">
              Linux Wayland 驱动策略（确认项 B）
            </label>
          </div>
          <select
            id="wayland-strategy-select"
            value={tuning.linuxWaylandStrategy}
            onChange={(e) =>
              onChangeWaylandStrategy(e.target.value as LinuxWaylandStrategy)
            }
            className="w-full rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 pl-3 pr-8 py-1.5 text-xs font-medium text-slate-900 dark:text-slate-100 focus:outline-none focus:ring-2 focus:ring-blue-600"
          >
            <option value="portal-and-uinput-fallback">
              推荐：Wayland (libei) + uinput + X11 双栈
            </option>
            <option value="portal-only">
              仅标准 XDG InputCapture Portal (libei)
            </option>
          </select>
          <div className="text-xs text-slate-500 dark:text-slate-400 truncate">
            当前生效栈：{activeDriverName}
          </div>
        </div>

        <div className="p-5 space-y-1">
          <div className="flex items-center gap-2 text-xs text-slate-500 dark:text-slate-400">
            <Terminal className="w-3.5 h-3.5 text-blue-600 dark:text-blue-400 shrink-0" />
            <span>GitHub Actions 云端四端构建</span>
          </div>
          <div className="font-mono text-xs text-slate-900 dark:text-slate-100 bg-slate-50 dark:bg-slate-950 border border-slate-200/80 dark:border-slate-800 rounded px-2.5 py-1.5 truncate">
            .github/workflows/build.yml
          </div>
          <div className="text-xs text-slate-500 dark:text-slate-400 truncate">
            推送代码自动产出 Win / Mac / Linux 安装包
          </div>
        </div>
      </section>

      {/* Two-Column Source Browser: Crate File List + Code Viewer */}
      <div className="grid grid-cols-1 xl:grid-cols-12 gap-6 items-start">
        <aside
          aria-label="Rust Cargo Workspace 工程文件树"
          className="xl:col-span-4 bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl overflow-hidden"
        >
          <div className="px-4 py-3.5 border-b border-slate-200 dark:border-slate-800 flex items-center justify-between">
            <h2 className="text-sm font-semibold text-slate-900 dark:text-slate-100">
              Cargo Workspace 模块文件
            </h2>
            <span className="text-xs text-slate-500 dark:text-slate-400 tabular-nums">
              {files.length} 项
            </span>
          </div>

          <div className="divide-y divide-slate-100 dark:divide-slate-800/80 max-h-[540px] overflow-y-auto">
            {files.map((item) => {
              const isSelected = item.path === selectedPath;
              return (
                <button
                  key={item.path}
                  type="button"
                  onClick={() => setSelectedPath(item.path)}
                  className={`w-full text-left px-4 py-3 transition-colors block ${
                    isSelected
                      ? 'bg-blue-50/80 dark:bg-blue-950/40'
                      : 'hover:bg-slate-50 dark:hover:bg-slate-800/40'
                  }`}
                >
                  <div className="flex items-center justify-between gap-2">
                    <span
                      className={`text-xs font-mono truncate ${
                        isSelected
                          ? 'font-semibold text-blue-700 dark:text-blue-300'
                          : 'text-slate-900 dark:text-slate-100'
                      }`}
                    >
                      {item.path}
                    </span>
                    <span className="text-[11px] font-mono text-slate-400 dark:text-slate-500 shrink-0 tabular-nums">
                      {item.lineCount} 行
                    </span>
                  </div>
                  <div className="text-[11px] text-slate-500 dark:text-slate-400 mt-0.5 truncate">
                    {item.category} · {item.summary}
                  </div>
                </button>
              );
            })}
          </div>
        </aside>

        <section
          aria-label="Rust 源码查看器"
          className="xl:col-span-8 min-w-0 bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl overflow-hidden"
        >
          <div className="px-5 py-3.5 border-b border-slate-200 dark:border-slate-800 flex flex-wrap items-center justify-between gap-3">
            <div className="min-w-0">
              <div className="flex items-center gap-2">
                <FileCode2 className="w-4 h-4 text-blue-600 dark:text-blue-400 shrink-0" />
                <span className="text-xs font-mono font-semibold text-slate-900 dark:text-slate-100 truncate">
                  {selectedPath}
                </span>
              </div>
              <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5 truncate">
                {fileSummary}
              </p>
            </div>

            <button
              type="button"
              onClick={handleCopySource}
              className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg border border-slate-200 dark:border-slate-700 text-slate-700 dark:text-slate-200 hover:bg-slate-50 dark:hover:bg-slate-800 transition-colors whitespace-nowrap shrink-0"
            >
              {copied ? <Check className="w-3.5 h-3.5 text-emerald-600" /> : <Copy className="w-3.5 h-3.5" />}
              <span>{copied ? '已复制源码' : '复制文件源码'}</span>
            </button>
          </div>

          <div className="bg-slate-950 text-slate-100 p-4 overflow-x-auto max-h-[540px] overflow-y-auto">
            {loadingFile ? (
              <div className="py-12 text-center text-xs font-mono text-slate-400">
                正在读取模块源码...
              </div>
            ) : (
              <pre className="text-xs font-mono leading-relaxed tabular-nums">
                <code>{fileContent}</code>
              </pre>
            )}
          </div>
        </section>
      </div>
    </div>
  );
};
