import React, { useRef, useState } from 'react';
import {
  Clipboard,
  FileUp,
  Send,
  Trash2,
  Check,
  FolderOpen,
} from 'lucide-react';
import {
  ClipboardPolicy,
  DeviceNode,
  TransferRecord,
  getOsLabel,
} from '../utils/kvmEngine';

interface ClipboardTransferViewProps {
  nodes: DeviceNode[];
  policy: ClipboardPolicy;
  onUpdatePolicy: (patch: Partial<ClipboardPolicy>) => void;
  transfers: TransferRecord[];
  onAddTransfer: (record: Omit<TransferRecord, 'id' | 'timestamp'>) => void;
  onClearTransfers: () => void;
}

export const ClipboardTransferView: React.FC<ClipboardTransferViewProps> = ({
  nodes,
  policy,
  onUpdatePolicy,
  transfers,
  onAddTransfer,
  onClearTransfers,
}) => {
  const primaryNode = nodes.find((n) => n.isPrimary) ?? nodes[0];
  const secondaryNodes = nodes.filter((n) => !n.isPrimary);

  const {
    syncPlainText,
    syncImages,
    enableDragDropFile,
    maxFileSizeMb,
    saveDir,
  } = policy;

  const [targetNodeId, setTargetNodeId] = useState<string>(
    secondaryNodes[0]?.id ?? nodes[0]?.id ?? ''
  );
  const [clipboardDraft, setClipboardDraft] = useState<string>(
    'RUSTFLAGS="-C target-cpu=native" cargo build --release'
  );
  const [logFilter, setLogFilter] = useState<'all' | 'clipboard' | 'file'>('all');
  const [isDraggingOver, setIsDraggingOver] = useState<boolean>(false);
  const [statusNotice, setStatusNotice] = useState<string | null>(null);

  const fileInputRef = useRef<HTMLInputElement | null>(null);

  const targetNode =
    nodes.find((n) => n.id === targetNodeId) ?? secondaryNodes[0] ?? nodes[0];

  const formatBytes = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  const handleBroadcastClipboard = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = clipboardDraft.trim();
    if (!trimmed) {
      setStatusNotice('请输入要跨设备同步的剪贴板文本内容。');
      return;
    }
    if (!syncPlainText) {
      setStatusNotice('纯文本剪贴板同步当前已关闭，请先在左侧开启。');
      return;
    }
    onAddTransfer({
      type: 'clipboard-text',
      title: trimmed.length > 52 ? `${trimmed.slice(0, 52)}...` : trimmed,
      sizeLabel: formatBytes(new TextEncoder().encode(trimmed).length),
      fromNodeName: primaryNode?.name ?? '本机主屏',
      toNodeName: `局域网全部在线节点 (${nodes.length} 台)`,
      encrypted: true,
      status: 'completed',
    });
    setStatusNotice('剪贴板内容已通过 TLS 1.3 可靠流同步至所有已连接屏幕。');
  };

  const triggerFileDrop = (fileName: string, sizeLabel: string) => {
    if (!enableDragDropFile) {
      setStatusNotice('跨屏幕文件拖拽传输当前处于关闭状态，请先开启开关。');
      return;
    }
    if (!targetNode) return;
    onAddTransfer({
      type: 'file-drop',
      title: fileName,
      sizeLabel,
      fromNodeName: primaryNode?.name ?? '本机主屏',
      toNodeName: targetNode.name,
      encrypted: true,
      status: 'completed',
    });
    setStatusNotice(
      `文件「${fileName}」已通过 QUIC 分块加密流投递至 ${targetNode.name} (${saveDir})。`
    );
  };

  const handleFilesSelected = (files: FileList | null) => {
    if (!files || files.length === 0) return;
    Array.from(files).forEach((file) => {
      triggerFileDrop(file.name, formatBytes(file.size));
    });
  };

  const filteredTransfers = transfers.filter((item) => {
    if (logFilter === 'all') return true;
    if (logFilter === 'file') return item.type === 'file-drop';
    return item.type === 'clipboard-text' || item.type === 'clipboard-image';
  });

  return (
    <div className="space-y-6">
      <div className="pb-5 border-b border-slate-200 dark:border-slate-800">
        <h1 className="text-xl font-semibold tracking-tight text-slate-900 dark:text-slate-100">
          跨设备剪贴板同步与文件拖拽传输
        </h1>
        <p className="text-sm text-slate-600 dark:text-slate-400 mt-1">
          剪贴板文本与截图自动双向镜像；按住鼠标左键将文件拖过屏幕边界即可触发局域网点对点 QUIC 加密分块直传。
        </p>
      </div>

      {statusNotice && (
        <div
          role="status"
          className="px-4 py-2.5 rounded-lg border border-blue-200 dark:border-blue-900 bg-blue-50/70 dark:bg-blue-950/30 text-xs text-slate-800 dark:text-slate-200 flex items-center justify-between gap-4"
        >
          <span>{statusNotice}</span>
          <button
            type="button"
            onClick={() => setStatusNotice(null)}
            className="text-slate-500 hover:text-slate-800 dark:text-slate-400 whitespace-nowrap shrink-0"
          >
            知道了
          </button>
        </div>
      )}

      <div className="grid grid-cols-1 xl:grid-cols-12 gap-6 items-start">
        {/* Left 5 Cols: Sync Policy & Strategy */}
        <section
          aria-label="剪贴板与文件传输策略设置"
          className="xl:col-span-5 bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl divide-y divide-slate-200 dark:divide-slate-800"
        >
          <div className="p-5 space-y-3.5">
            <h2 className="text-base font-semibold text-slate-900 dark:text-slate-100">
              剪贴板与拖拽能力开关
            </h2>

            {[
              {
                id: 'toggle-text',
                title: '同步纯文本与代码片段 (UTF-8 / HTML)',
                desc: '复制后焦点切入目标屏幕时零感按需拉取',
                checked: syncPlainText,
                onToggle: () => onUpdatePolicy({ syncPlainText: !syncPlainText }),
              },
              {
                id: 'toggle-img',
                title: '同步剪贴板位图与屏幕截图 (PNG)',
                desc: '跨系统自动转换 NSPasteboard / CF_DIB / Wayland MIME',
                checked: syncImages,
                onToggle: () => onUpdatePolicy({ syncImages: !syncImages }),
              },
              {
                id: 'toggle-drop',
                title: '允许鼠标跨屏幕边缘直接拖拽文件传输',
                desc: '使用 BLAKE3 块级完整性校验，断流支持自动续传',
                checked: enableDragDropFile,
                onToggle: () => onUpdatePolicy({ enableDragDropFile: !enableDragDropFile }),
              },
            ].map((item) => (
              <div
                key={item.id}
                className="flex items-start justify-between gap-4 py-1.5"
              >
                <div>
                  <div className="text-xs font-medium text-slate-900 dark:text-slate-100">
                    {item.title}
                  </div>
                  <div className="text-xs text-slate-500 dark:text-slate-400 mt-0.5">
                    {item.desc}
                  </div>
                </div>
                <button
                  type="button"
                  onClick={item.onToggle}
                  aria-label={`切换 ${item.title}`}
                  className={`inline-flex items-center gap-1 px-2.5 py-1 rounded-md border text-xs font-medium transition-colors whitespace-nowrap shrink-0 ${
                    item.checked
                      ? 'border-blue-200 dark:border-blue-800 bg-blue-50 dark:bg-blue-950/40 text-blue-700 dark:text-blue-300'
                      : 'border-slate-200 dark:border-slate-700 text-slate-500 dark:text-slate-400'
                  }`}
                >
                  {item.checked && <Check className="w-3 h-3" />}
                  {item.checked ? '已开启' : '已关闭'}
                </button>
              </div>
            ))}
          </div>

          <div className="p-5 space-y-4">
            <div>
              <label
                htmlFor="max-file-limit"
                className="block text-xs font-medium text-slate-700 dark:text-slate-300 mb-1.5"
              >
                单次拖拽传输大小上限
              </label>
              <select
                id="max-file-limit"
                value={maxFileSizeMb}
                onChange={(e) => onUpdatePolicy({ maxFileSizeMb: Number(e.target.value) })}
                className="w-full rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-950 pl-3 pr-8 py-2 text-xs font-mono text-slate-900 dark:text-slate-100 tabular-nums"
              >
                <option value={100}>100 MB (适合文档与常规压缩包)</option>
                <option value={500}>500 MB (推荐默认值 · 局域网秒传)</option>
                <option value={2048}>2048 MB (大体积构建产物与镜像)</option>
              </select>
            </div>

            <div>
              <label
                htmlFor="receive-folder-input"
                className="block text-xs font-medium text-slate-700 dark:text-slate-300 mb-1.5"
              >
                落盘默认目录（未拖入具体窗口时保存位置）
              </label>
              <div className="relative">
                <FolderOpen className="w-3.5 h-3.5 text-slate-400 absolute left-3 top-1/2 -translate-y-1/2 pointer-events-none" />
                <input
                  id="receive-folder-input"
                  type="text"
                  value={saveDir}
                  onChange={(e) => onUpdatePolicy({ saveDir: e.target.value })}
                  className="w-full pl-8 pr-3 py-2 rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs font-mono text-slate-900 dark:text-slate-100"
                />
              </div>
            </div>
          </div>
        </section>

        {/* Right 7 Cols: Interactive File Drop & Shared Clipboard Tester */}
        <div className="xl:col-span-7 space-y-6">
          {/* Drag and Drop File Simulator */}
          <section
            aria-label="跨屏幕文件拖拽投递演示"
            className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl p-5 space-y-4"
          >
            <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
              <div>
                <h2 className="text-base font-semibold text-slate-900 dark:text-slate-100">
                  跨屏幕拖拽文件投递台
                </h2>
                <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5">
                  选择接收主机并拖入本地文件（或点击下方示例文件一键测试加密传输流水）。
                </p>
              </div>

              <div className="shrink-0">
                <label htmlFor="drop-target-node" className="sr-only">
                  选择拖拽接收目标电脑
                </label>
                <select
                  id="drop-target-node"
                  value={targetNode?.id ?? ''}
                  onChange={(e) => setTargetNodeId(e.target.value)}
                  className="rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 px-3 py-1.5 text-xs text-slate-900 dark:text-slate-100"
                >
                  {nodes.map((n) => (
                    <option key={n.id} value={n.id}>
                      投递目标：{n.name} ({getOsLabel(n.os)})
                    </option>
                  ))}
                </select>
              </div>
            </div>

            <div
              onDragOver={(e) => {
                e.preventDefault();
                setIsDraggingOver(true);
              }}
              onDragLeave={() => setIsDraggingOver(false)}
              onDrop={(e) => {
                e.preventDefault();
                setIsDraggingOver(false);
                handleFilesSelected(e.dataTransfer.files);
              }}
              className={`rounded-lg border border-dashed p-6 text-center transition-colors ${
                isDraggingOver
                  ? 'border-blue-600 bg-blue-50/50 dark:bg-blue-950/30'
                  : 'border-slate-300 dark:border-slate-700 bg-slate-50/60 dark:bg-slate-950/40'
              }`}
            >
              <FileUp className="w-6 h-6 text-slate-400 mx-auto mb-2" />
              <p className="text-xs font-medium text-slate-700 dark:text-slate-200">
                将文件拖拽至此处模拟跨屏幕边界松开鼠标，或选择本地文件发送至「{targetNode?.name}」
              </p>
              <p className="text-xs text-slate-500 dark:text-slate-400 mt-1 tabular-nums">
                端到端 ChaCha20-Poly1305 加密 · 上限 {maxFileSizeMb} MB
              </p>

              <input
                ref={fileInputRef}
                type="file"
                multiple
                onChange={(e) => handleFilesSelected(e.target.files)}
                className="hidden"
              />

              <div className="mt-4 flex flex-wrap items-center justify-center gap-2">
                <button
                  type="button"
                  onClick={() => fileInputRef.current?.click()}
                  className="px-3.5 py-1.5 text-xs font-medium rounded-lg bg-blue-600 text-white hover:bg-blue-700 transition-colors whitespace-nowrap shrink-0"
                >
                  选择本地文件投递
                </button>
                <button
                  type="button"
                  onClick={() => triggerFileDrop('spankvm-driver-spec.pdf', '3.4 MB')}
                  className="px-3 py-1.5 text-xs font-medium rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-900 text-slate-700 dark:text-slate-200 hover:bg-slate-50 dark:hover:bg-slate-800 transition-colors whitespace-nowrap shrink-0"
                >
                  模拟拖拽 spankvm-driver-spec.pdf
                </button>
                <button
                  type="button"
                  onClick={() => triggerFileDrop('macos-win-keymap.json', '12.8 KB')}
                  className="px-3 py-1.5 text-xs font-medium rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-900 text-slate-700 dark:text-slate-200 hover:bg-slate-50 dark:hover:bg-slate-800 transition-colors whitespace-nowrap shrink-0"
                >
                  模拟拖拽 macos-win-keymap.json
                </button>
              </div>
            </div>

            {/* Live Clipboard Push Form */}
            <form onSubmit={handleBroadcastClipboard} className="pt-2 space-y-2">
              <label
                htmlFor="shared-clipboard-input"
                className="block text-xs font-medium text-slate-700 dark:text-slate-300"
              >
                实时共享剪贴板缓冲区测试（模拟主屏 Cmd/Ctrl + C 复制同步）
              </label>
              <div className="flex gap-2">
                <input
                  id="shared-clipboard-input"
                  type="text"
                  value={clipboardDraft}
                  onChange={(e) => setClipboardDraft(e.target.value)}
                  className="flex-1 rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-950 px-3 py-2 text-xs font-mono text-slate-900 dark:text-slate-100"
                />
                <button
                  type="submit"
                  className="inline-flex items-center gap-1.5 px-4 py-2 text-xs font-medium rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-900 dark:bg-slate-100 text-white dark:text-slate-900 hover:opacity-90 transition-opacity whitespace-nowrap shrink-0"
                >
                  <Send className="w-3.5 h-3.5" />
                  同步剪贴板
                </button>
              </div>
            </form>
          </section>
        </div>
      </div>

      {/* Transfer & Sync Activity Table */}
      <section
        aria-label="跨设备同步与文件传输记录"
        className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl overflow-hidden"
      >
        <div className="p-5 border-b border-slate-200 dark:border-slate-800 flex flex-wrap items-center justify-between gap-3">
          <div className="flex items-center gap-2">
            <Clipboard className="w-4 h-4 text-slate-500 shrink-0" />
            <h2 className="text-base font-semibold text-slate-900 dark:text-slate-100">
              最近剪贴板同步与文件拖拽流水 ({filteredTransfers.length})
            </h2>
          </div>

          <div className="flex items-center gap-2">
            <div className="flex items-center gap-1 p-1 rounded-lg bg-slate-100 dark:bg-slate-800">
              {(
                [
                  { id: 'all', label: '全部记录' },
                  { id: 'file', label: '文件拖拽' },
                  { id: 'clipboard', label: '剪贴板' },
                ] as const
              ).map((tab) => (
                <button
                  key={tab.id}
                  type="button"
                  onClick={() => setLogFilter(tab.id)}
                  className={`px-2.5 py-1 text-xs font-medium rounded-md transition-colors whitespace-nowrap shrink-0 ${
                    logFilter === tab.id
                      ? 'bg-white dark:bg-slate-900 text-slate-900 dark:text-slate-100 shadow-xs'
                      : 'text-slate-600 dark:text-slate-400'
                  }`}
                >
                  {tab.label}
                </button>
              ))}
            </div>

            {transfers.length > 0 && (
              <button
                type="button"
                onClick={onClearTransfers}
                className="inline-flex items-center gap-1 px-2.5 py-1.5 text-xs text-slate-500 hover:text-red-600 rounded-lg border border-slate-200 dark:border-slate-700 hover:bg-red-50 dark:hover:bg-red-950/30 transition-colors whitespace-nowrap shrink-0"
              >
                <Trash2 className="w-3.5 h-3.5" />
                清空记录
              </button>
            )}
          </div>
        </div>

        {filteredTransfers.length === 0 ? (
          <div className="p-10 text-center space-y-2">
            <p className="text-sm font-medium text-slate-700 dark:text-slate-300">
              暂无跨设备传输记录
            </p>
            <p className="text-xs text-slate-500 dark:text-slate-400">
              在上方点击「模拟拖拽 spankvm-driver-spec.pdf」或「同步剪贴板」即可查看实时加密传输流水。
            </p>
          </div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-left border-collapse text-xs">
              <thead>
                <tr className="border-b border-slate-200 dark:border-slate-800 text-slate-500 dark:text-slate-400">
                  <th className="py-3 px-5 font-medium">内容摘要 / 文件名</th>
                  <th className="py-3 px-4 font-medium">类型与体积</th>
                  <th className="py-3 px-4 font-medium">流向路径</th>
                  <th className="py-3 px-4 font-medium">安全状态</th>
                  <th className="py-3 px-5 font-medium text-right">完成时间</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-200 dark:divide-slate-800">
                {filteredTransfers.map((tr) => (
                  <tr
                    key={tr.id}
                    className="hover:bg-slate-50/70 dark:hover:bg-slate-800/40 transition-colors"
                  >
                    <td className="py-3 px-5 font-mono text-slate-900 dark:text-slate-100">
                      {tr.title}
                    </td>
                    <td className="py-3 px-4 text-slate-600 dark:text-slate-300 tabular-nums">
                      {tr.type === 'file-drop'
                        ? '跨屏文件拖拽'
                        : tr.type === 'clipboard-image'
                        ? '剪贴板图像'
                        : '剪贴板文本'}{' '}
                      · {tr.sizeLabel}
                    </td>
                    <td className="py-3 px-4 text-slate-700 dark:text-slate-300">
                      {tr.fromNodeName} → {tr.toNodeName}
                    </td>
                    <td className="py-3 px-4 text-slate-600 dark:text-slate-400 font-mono tabular-nums whitespace-nowrap">
                      TLS 1.3 · {tr.digestHex ? `摘要 ${tr.digestHex.slice(0, 8)}` : 'BLAKE3 已校验'}
                    </td>
                    <td className="py-3 px-5 text-right font-mono text-slate-500 dark:text-slate-400 tabular-nums">
                      {tr.timestamp}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>
    </div>
  );
};
