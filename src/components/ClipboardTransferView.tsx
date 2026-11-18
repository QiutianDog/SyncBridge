import React, { useRef, useState } from 'react';
import {
  Clipboard,
  FileUp,
  Send,
  Trash2,
  Check,
  FolderOpen,
  ExternalLink,
} from 'lucide-react';
import {
  ClipboardPolicy,
  DeviceNode,
  TransferRecord,
  encodeArrayBufferToBase64,
  getOsLabel,
} from '../utils/kvmEngine';

interface ClipboardTransferViewProps {
  nodes: DeviceNode[];
  policy: ClipboardPolicy;
  onUpdatePolicy: (patch: Partial<ClipboardPolicy>) => void;
  transfers: TransferRecord[];
  onBroadcastClipboard: (text: string) => Promise<{ ok: boolean; error?: string }>;
  onSendFilePayload: (
    fileName: string,
    targetNodeId: string,
    base64Data: string
  ) => Promise<{ ok: boolean; error?: string }>;
  onOpenReceiveDir: () => Promise<void>;
  onClearTransfers: () => void;
}

export const ClipboardTransferView: React.FC<ClipboardTransferViewProps> = ({
  nodes,
  policy,
  onUpdatePolicy,
  transfers,
  onBroadcastClipboard,
  onSendFilePayload,
  onOpenReceiveDir,
  onClearTransfers,
}) => {
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
  const [clipboardDraft, setClipboardDraft] = useState<string>('');
  const [logFilter, setLogFilter] = useState<'all' | 'clipboard' | 'file'>('all');
  const [isDraggingOver, setIsDraggingOver] = useState<boolean>(false);
  const [isTransferring, setIsTransferring] = useState<boolean>(false);
  const [statusNotice, setStatusNotice] = useState<{
    tone: 'info' | 'error';
    text: string;
  } | null>(null);

  const fileInputRef = useRef<HTMLInputElement | null>(null);

  const targetNode =
    nodes.find((n) => n.id === targetNodeId) ?? secondaryNodes[0] ?? nodes[0];

  const handleClipboardSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = clipboardDraft.trim();
    if (!trimmed) {
      setStatusNotice({
        tone: 'error',
        text: '请输入要写入系统剪贴板并跨设备同步的文本内容。',
      });
      return;
    }
    const res = await onBroadcastClipboard(trimmed);
    if (!res.ok) {
      setStatusNotice({
        tone: 'error',
        text: res.error ?? '剪贴板同步失败。',
      });
      return;
    }
    setClipboardDraft('');
    setStatusNotice({
      tone: 'info',
      text: '文本已写入本机剪贴板并通过 QUIC TLS 1.3 加密流同步至在线互联主机。',
    });
  };

  const handleFilesSelected = async (files: FileList | null) => {
    if (!files || files.length === 0) return;
    if (!enableDragDropFile) {
      setStatusNotice({
        tone: 'error',
        text: '文件拖拽传输开关当前已关闭，请先在左侧开启后再发送文件。',
      });
      return;
    }
    if (!targetNode) return;

    setIsTransferring(true);
    try {
      for (const file of Array.from(files)) {
        if (file.size > maxFileSizeMb * 1024 * 1024) {
          setStatusNotice({
            tone: 'error',
            text: `文件「${file.name}」超出单次传输上限 (${maxFileSizeMb} MB)。`,
          });
          continue;
        }
        const buf = await file.arrayBuffer();
        const b64 = encodeArrayBufferToBase64(buf);
        const res = await onSendFilePayload(file.name, targetNode.id, b64);
        if (!res.ok) {
          setStatusNotice({
            tone: 'error',
            text: res.error ?? `发送文件「${file.name}」失败。`,
          });
          return;
        }
        setStatusNotice({
          tone: 'info',
          text: `文件「${file.name}」已完成 BLAKE3 分块校验并投递至 ${targetNode.name}。`,
        });
      }
    } finally {
      setIsTransferring(false);
      if (fileInputRef.current) {
        fileInputRef.current.value = '';
      }
    }
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
          后台守护进程自动监听各端操作系统剪贴板变更并双向同步；拖入本地文件可通过 QUIC 加密流分块直传至指定电脑并执行 BLAKE3 完整性校验。
        </p>
      </div>

      {statusNotice && (
        <div
          role="status"
          className={`px-4 py-2.5 rounded-lg border text-xs flex items-center justify-between gap-4 ${
            statusNotice.tone === 'error'
              ? 'border-red-200 dark:border-red-900 bg-red-50 dark:bg-red-950/30 text-red-700 dark:text-red-300'
              : 'border-blue-200 dark:border-blue-900 bg-blue-50/70 dark:bg-blue-950/30 text-slate-800 dark:text-slate-200'
          }`}
        >
          <span>{statusNotice.text}</span>
          <button
            type="button"
            onClick={() => setStatusNotice(null)}
            className="text-slate-500 hover:text-slate-800 dark:text-slate-400 whitespace-nowrap shrink-0"
          >
            关闭提示
          </button>
        </div>
      )}

      <div className="grid grid-cols-1 xl:grid-cols-12 gap-6 items-start">
        {/* Left 5 Cols: Sync Policy & Receive Folder */}
        <section
          aria-label="剪贴板与文件传输策略设置"
          className="xl:col-span-5 bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl divide-y divide-slate-200 dark:divide-slate-800"
        >
          <div className="p-5 space-y-3.5">
            <h2 className="text-base font-semibold text-slate-900 dark:text-slate-100">
              自动同步与文件接收策略
            </h2>

            {[
              {
                id: 'toggle-text',
                title: '自动同步系统纯文本剪贴板 (UTF-8)',
                desc: '任意一台电脑复制文本后，其他在线电脑可直接粘贴',
                checked: syncPlainText,
                onToggle: () => onUpdatePolicy({ syncPlainText: !syncPlainText }),
              },
              {
                id: 'toggle-img',
                title: '同步剪贴板图像与屏幕截图 (RGBA / PNG)',
                desc: '跨 macOS NSPasteboard / Windows / Wayland 自动转换格式',
                checked: syncImages,
                onToggle: () => onUpdatePolicy({ syncImages: !syncImages }),
              },
              {
                id: 'toggle-drop',
                title: '启用局域网加密文件拖拽直传',
                desc: '64 KB 分块流式传输，逐块验证 BLAKE3 哈希摘要',
                checked: enableDragDropFile,
                onToggle: () =>
                  onUpdatePolicy({ enableDragDropFile: !enableDragDropFile }),
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
                单次文件传输大小上限
              </label>
              <select
                id="max-file-limit"
                value={maxFileSizeMb}
                onChange={(e) =>
                  onUpdatePolicy({ maxFileSizeMb: Number(e.target.value) })
                }
                className="w-full rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-950 pl-3 pr-8 py-2 text-xs font-mono text-slate-900 dark:text-slate-100 tabular-nums"
              >
                <option value={100}>100 MB (文档与常规代码包)</option>
                <option value={500}>500 MB (推荐默认值)</option>
                <option value={2048}>2048 MB (大体积安装包与镜像)</option>
              </select>
            </div>

            <div>
              <div className="flex items-center justify-between mb-1.5">
                <label
                  htmlFor="receive-folder-input"
                  className="block text-xs font-medium text-slate-700 dark:text-slate-300"
                >
                  本机接收文件保存目录
                </label>
                <button
                  type="button"
                  onClick={onOpenReceiveDir}
                  className="inline-flex items-center gap-1 text-xs text-blue-600 dark:text-blue-400 hover:underline whitespace-nowrap shrink-0"
                >
                  <ExternalLink className="w-3 h-3" />
                  <span>打开接收目录</span>
                </button>
              </div>
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

        {/* Right 7 Cols: Real File Dropzone & Direct Clipboard Broadcast */}
        <div className="xl:col-span-7 space-y-6">
          <section
            aria-label="跨设备文件拖拽发送与剪贴板推送"
            className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl p-5 space-y-4"
          >
            <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
              <div>
                <h2 className="text-base font-semibold text-slate-900 dark:text-slate-100">
                  拖拽发送本地文件至目标电脑
                </h2>
                <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5">
                  将任意本地文件拖入下方区域（或点击选择文件），通过加密通道直接发送至选定设备的接收目录。
                </p>
              </div>

              <div className="shrink-0">
                <label htmlFor="drop-target-node" className="sr-only">
                  选择接收目标电脑
                </label>
                <select
                  id="drop-target-node"
                  value={targetNode?.id ?? ''}
                  onChange={(e) => setTargetNodeId(e.target.value)}
                  className="rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 px-3 py-1.5 text-xs text-slate-900 dark:text-slate-100"
                >
                  {nodes.map((n) => (
                    <option key={n.id} value={n.id}>
                      接收设备：{n.name} ({getOsLabel(n.os)})
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
                将本地文件拖拽至此处松开，直接加密发送至「{targetNode?.name}」
              </p>
              <p className="text-xs text-slate-500 dark:text-slate-400 mt-1 tabular-nums">
                QUIC TLS 1.3 + BLAKE3 分块完整性校验 · 单文件上限 {maxFileSizeMb} MB
              </p>

              <input
                ref={fileInputRef}
                type="file"
                multiple
                onChange={(e) => handleFilesSelected(e.target.files)}
                className="hidden"
              />

              <div className="mt-4 flex items-center justify-center">
                <button
                  type="button"
                  disabled={isTransferring}
                  onClick={() => fileInputRef.current?.click()}
                  className="px-4 py-2 text-xs font-medium rounded-lg bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-50 transition-colors whitespace-nowrap shrink-0"
                >
                  {isTransferring ? '正在加密传输中...' : '选择本地文件发送'}
                </button>
              </div>
            </div>

            {/* Direct OS Clipboard Push Form */}
            <form onSubmit={handleClipboardSubmit} className="pt-2 space-y-2">
              <label
                htmlFor="shared-clipboard-input"
                className="block text-xs font-medium text-slate-700 dark:text-slate-300"
              >
                主动推送文本至全局剪贴板（写入本机系统剪贴板并立即广播至所有在线设备）
              </label>
              <div className="flex gap-2">
                <input
                  id="shared-clipboard-input"
                  type="text"
                  placeholder="输入文本、终端命令或链接，一键同步到所有电脑剪贴板..."
                  value={clipboardDraft}
                  onChange={(e) => setClipboardDraft(e.target.value)}
                  className="flex-1 rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-950 px-3 py-2 text-xs font-mono text-slate-900 dark:text-slate-100 focus:outline-none focus:ring-2 focus:ring-blue-600"
                />
                <button
                  type="submit"
                  className="inline-flex items-center gap-1.5 px-4 py-2 text-xs font-medium rounded-lg bg-slate-900 dark:bg-slate-100 text-white dark:text-slate-900 hover:opacity-90 transition-opacity whitespace-nowrap shrink-0"
                >
                  <Send className="w-3.5 h-3.5" />
                  <span>同步剪贴板</span>
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
            <h2 className="text-base font-semibold text-slate-900 dark:text-slate-100 tabular-nums">
              跨设备剪贴板与文件传输记录 ({filteredTransfers.length})
            </h2>
          </div>

          <div className="flex items-center gap-2">
            <div className="flex items-center gap-1 p-1 rounded-lg bg-slate-100 dark:bg-slate-800">
              {(
                [
                  { id: 'all', label: '全部记录' },
                  { id: 'file', label: '文件传输' },
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
                <span>清空记录</span>
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
              在任意已连接电脑上复制文本，或将本地文件拖入上方发送区后，传输记录与 BLAKE3 校验摘要会实时显示在此处。
            </p>
          </div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-left border-collapse text-xs">
              <thead>
                <tr className="border-b border-slate-200 dark:border-slate-800 text-slate-500 dark:text-slate-400">
                  <th className="py-3 px-5 font-medium whitespace-nowrap">内容摘要 / 文件名</th>
                  <th className="py-3 px-4 font-medium whitespace-nowrap">类型与大小</th>
                  <th className="py-3 px-4 font-medium whitespace-nowrap">传输方向</th>
                  <th className="py-3 px-4 font-medium whitespace-nowrap">加密与完整性校验</th>
                  <th className="py-3 px-5 font-medium text-right whitespace-nowrap">时间</th>
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
                    <td className="py-3 px-4 text-slate-600 dark:text-slate-300 tabular-nums whitespace-nowrap">
                      {tr.type === 'file-drop'
                        ? '文件传输'
                        : tr.type === 'clipboard-image'
                          ? '剪贴板图像'
                          : '剪贴板文本'}{' '}
                      · {tr.sizeLabel}
                    </td>
                    <td className="py-3 px-4 text-slate-700 dark:text-slate-300 whitespace-nowrap">
                      {tr.fromNodeName} → {tr.toNodeName}
                    </td>
                    <td className="py-3 px-4 text-slate-600 dark:text-slate-400 font-mono tabular-nums whitespace-nowrap">
                      TLS 1.3 · {tr.digestHex ? `摘要 ${tr.digestHex.slice(0, 8)}` : 'BLAKE3 已校验'}
                    </td>
                    <td className="py-3 px-5 text-right font-mono text-slate-500 dark:text-slate-400 tabular-nums whitespace-nowrap">
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
