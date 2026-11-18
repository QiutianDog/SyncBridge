import React, { useState } from 'react';
import {
  Monitor,
  Laptop,
  Server,
  Crown,
  ArrowUp,
  ArrowDown,
  ArrowLeft,
  ArrowRight,
  MousePointer,
  Keyboard,
  Plus,
  ShieldAlert,
  Pause,
  Play,
  RotateCcw,
} from 'lucide-react';
import {
  DeviceNode,
  GRID_COLS,
  GRID_ROWS,
  LocalSystemStatus,
  MAX_DEVICES,
  VirtualCursorState,
  computeNeighbors,
  getOsLabel,
} from '../utils/kvmEngine';

interface TopologyWorkspaceProps {
  nodes: DeviceNode[];
  selectedNodeId: string;
  onSelectNode: (id: string) => void;
  onSetPrimary: (id: string) => void;
  onMoveNode: (
    nodeId: string,
    col: number,
    row: number
  ) => Promise<{ ok: boolean; error?: string; swappedWith?: string }>;
  onUpdateNodeRefreshRate: (nodeId: string, hz: DeviceNode['refreshRateHz']) => void;
  cursor: VirtualCursorState;
  onJumpCursorToNode: (nodeId: string) => void;
  onEmergencyReturnPrimary: () => void;
  daemonActive: boolean;
  onToggleDaemonActive: () => void;
  onNavigateToPairing: () => void;
  hidPollingHz: 500 | 1000;
  onChangeHidPollingHz: (hz: 500 | 1000) => void;
  edgeDelayMs: number;
  onChangeEdgeDelayMs: (ms: number) => void;
  releaseHotkey: string;
  systemStatus: LocalSystemStatus;
  onRequestInputPermission: () => void;
}

export const TopologyWorkspace: React.FC<TopologyWorkspaceProps> = ({
  nodes,
  selectedNodeId,
  onSelectNode,
  onSetPrimary,
  onMoveNode,
  onUpdateNodeRefreshRate,
  cursor,
  onJumpCursorToNode,
  onEmergencyReturnPrimary,
  daemonActive,
  onToggleDaemonActive,
  onNavigateToPairing,
  hidPollingHz,
  onChangeHidPollingHz,
  edgeDelayMs,
  onChangeEdgeDelayMs,
  releaseHotkey,
  systemStatus,
  onRequestInputPermission,
}) => {
  const [topologyFeedback, setTopologyFeedback] = useState<{
    tone: 'info' | 'error';
    message: string;
  } | null>(null);
  const [draggedNodeId, setDraggedNodeId] = useState<string | null>(null);

  const selectedNode = nodes.find((n) => n.id === selectedNodeId) ?? nodes[0];
  const activeFocusNode = nodes.find((n) => n.id === cursor.activeNodeId) ?? nodes[0];
  const primaryNode = nodes.find((n) => n.isPrimary) ?? nodes[0];
  const selectedNeighbors = selectedNode ? computeNeighbors(nodes, selectedNode.id) : {};

  const handleCellAction = async (targetNodeId: string, col: number, row: number) => {
    const res = await onMoveNode(targetNodeId, col, row);
    if (!res.ok && res.error) {
      setTopologyFeedback({ tone: 'error', message: res.error });
    } else if (res.swappedWith) {
      setTopologyFeedback({
        tone: 'info',
        message: `已将屏幕与「${res.swappedWith}」互换上下左右相对位置，并同步至跨屏路由核心。`,
      });
    } else {
      setTopologyFeedback({
        tone: 'info',
        message: `屏幕位置已更新至网格 (${col + 1}, ${row + 1})，边缘跨屏路由已实时生效。`,
      });
    }
  };

  const renderOsIcon = (os: DeviceNode['os']) => {
    if (os === 'macos') return <Laptop className="w-4 h-4 text-slate-600 dark:text-slate-300 shrink-0" />;
    if (os === 'windows') return <Monitor className="w-4 h-4 text-slate-600 dark:text-slate-300 shrink-0" />;
    return <Server className="w-4 h-4 text-slate-600 dark:text-slate-300 shrink-0" />;
  };

  return (
    <div className="space-y-6">
      {/* OS Input Capture Permission Alert Banner (shown only when OS permission is missing) */}
      {!systemStatus.inputPermissionGranted && (
        <div
          role="alert"
          className="px-4 py-3 rounded-xl border border-amber-200 dark:border-amber-900 bg-amber-50/80 dark:bg-amber-950/30 flex flex-col sm:flex-row sm:items-center justify-between gap-3"
        >
          <div className="flex items-start gap-2.5 text-xs text-amber-900 dark:text-amber-200">
            <ShieldAlert className="w-4 h-4 text-amber-600 dark:text-amber-400 shrink-0 mt-0.5" />
            <div>
              <span className="font-semibold">尚未授予系统全局键鼠控制权限：</span>
              <span>
                请在系统设置「隐私与安全性 → 辅助功能」中允许 SpanKVM 控制电脑，授权后即可捕获物理键鼠并跨屏幕控制其他主机。
              </span>
            </div>
          </div>
          <button
            type="button"
            onClick={onRequestInputPermission}
            className="px-3.5 py-1.5 text-xs font-medium rounded-lg bg-amber-700 text-white hover:bg-amber-800 transition-colors whitespace-nowrap shrink-0"
          >
            前往系统权限设置
          </button>
        </div>
      )}

      {/* Section Header & Core Controls */}
      <div className="flex flex-col lg:flex-row lg:items-center lg:justify-between gap-4 pb-5 border-b border-slate-200 dark:border-slate-800">
        <div>
          <h1 className="text-xl font-semibold tracking-tight text-slate-900 dark:text-slate-100">
            多屏幕空间拓扑与跨屏焦点控制
          </h1>
          <p className="text-sm text-slate-600 dark:text-slate-400 mt-1">
            编排物理键鼠所在的主屏幕及最多 {MAX_DEVICES} 台电脑的上下左右相邻关系；鼠标滑过屏幕边界时，键盘控制权自动同步切换。
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-2.5 shrink-0">
          <button
            type="button"
            onClick={onToggleDaemonActive}
            className={`inline-flex items-center gap-1.5 px-3.5 py-2 text-xs font-medium rounded-lg border transition-colors whitespace-nowrap shrink-0 ${
              daemonActive
                ? 'border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-900 text-slate-700 dark:text-slate-200 hover:bg-slate-50 dark:hover:bg-slate-800'
                : 'border-amber-300 dark:border-amber-800 bg-amber-50 dark:bg-amber-950/40 text-amber-800 dark:text-amber-300'
            }`}
          >
            {daemonActive ? (
              <>
                <Pause className="w-3.5 h-3.5" />
                <span>暂停跨屏共享</span>
              </>
            ) : (
              <>
                <Play className="w-3.5 h-3.5" />
                <span>恢复跨屏共享</span>
              </>
            )}
          </button>

          <button
            type="button"
            onClick={() => {
              onEmergencyReturnPrimary();
              setTopologyFeedback({
                tone: 'info',
                message: '已强制释放远端修饰键并将键鼠焦点收回主屏幕中心。',
              });
            }}
            className="inline-flex items-center gap-1.5 px-3.5 py-2 text-xs font-medium rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-900 text-slate-700 dark:text-slate-200 hover:bg-slate-50 dark:hover:bg-slate-800 transition-colors whitespace-nowrap shrink-0"
          >
            <RotateCcw className="w-3.5 h-3.5" />
            <span>收回本机焦点</span>
          </button>

          <button
            type="button"
            onClick={onNavigateToPairing}
            className="inline-flex items-center gap-1.5 px-4 py-2 text-xs font-medium rounded-lg bg-blue-600 text-white hover:bg-blue-700 transition-colors whitespace-nowrap shrink-0"
          >
            <Plus className="w-3.5 h-3.5" />
            <span>配对接入新电脑 ({nodes.length}/{MAX_DEVICES})</span>
          </button>
        </div>
      </div>

      {topologyFeedback && (
        <div
          role="status"
          className={`px-4 py-2.5 rounded-lg border text-xs flex items-center justify-between gap-4 ${
            topologyFeedback.tone === 'error'
              ? 'bg-red-50/80 dark:bg-red-950/30 border-red-200 dark:border-red-900 text-red-800 dark:text-red-200'
              : 'bg-blue-50/70 dark:bg-blue-950/30 border-blue-200 dark:border-blue-900 text-slate-800 dark:text-slate-200'
          }`}
        >
          <span>{topologyFeedback.message}</span>
          <button
            type="button"
            onClick={() => setTopologyFeedback(null)}
            className="text-slate-500 hover:text-slate-800 dark:text-slate-400 dark:hover:text-slate-200 whitespace-nowrap shrink-0"
          >
            关闭提示
          </button>
        </div>
      )}

      {/* Main 2-Column Workspace: Spatial Grid Canvas + Selected Screen Inspector */}
      <div className="grid grid-cols-1 xl:grid-cols-12 gap-6 items-start">
        {/* Left 8 Cols: 5x5 Spatial Matrix */}
        <section
          aria-label="屏幕上下左右空间排布画布"
          className="xl:col-span-8 min-w-0 bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl p-5"
        >
          <div className="flex flex-wrap items-center justify-between gap-3 pb-4 mb-4 border-b border-slate-100 dark:border-slate-800/80">
            <div>
              <h2 className="text-base font-semibold text-slate-900 dark:text-slate-100">
                屏幕排列画布（拖拽屏幕卡片或点击相邻槽位调整上下左右关系）
              </h2>
              <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5 tabular-nums">
                已配置节点：{nodes.length} / {MAX_DEVICES} 台 · 主屏幕：{primaryNode?.name ?? '未指定'} · 当前键鼠焦点：{activeFocusNode?.name ?? '无'}
              </p>
            </div>
            <div className="text-xs text-slate-500 dark:text-slate-400 tabular-nums">
              5×5 正交连通拓扑
            </div>
          </div>

          <div className="overflow-x-auto -mx-1 px-1">
            <div
              className="grid grid-cols-5 gap-2.5 select-none min-w-[540px]"
              data-testid="topology-grid"
            >
              {Array.from({ length: GRID_ROWS }).map((_, row) =>
                Array.from({ length: GRID_COLS }).map((__, col) => {
                  const occupant = nodes.find(
                    (n) => n.gridPos.col === col && n.gridPos.row === row
                  );
                  const isSelected = occupant?.id === selectedNode?.id;
                  const isCursorHere = occupant?.id === cursor.activeNodeId;
                  const isLocalMachine = occupant?.id === systemStatus.localNodeId;

                  if (occupant) {
                    return (
                      <div
                        key={`${col}-${row}`}
                        draggable
                        onDragStart={() => {
                          setDraggedNodeId(occupant.id);
                          onSelectNode(occupant.id);
                        }}
                        onDragOver={(e) => e.preventDefault()}
                        onDrop={(e) => {
                          e.preventDefault();
                          if (draggedNodeId && draggedNodeId !== occupant.id) {
                            handleCellAction(draggedNodeId, col, row);
                          }
                          setDraggedNodeId(null);
                        }}
                        className={`relative rounded-lg border p-2.5 transition-colors flex flex-col justify-between min-h-[122px] ${
                          isSelected
                            ? 'border-blue-600 dark:border-blue-500 bg-blue-50/50 dark:bg-blue-950/30'
                            : 'border-slate-200 dark:border-slate-800 bg-slate-50/60 dark:bg-slate-950/50 hover:border-slate-300 dark:hover:border-slate-700'
                        }`}
                      >
                        <button
                          type="button"
                          onClick={() => onSelectNode(occupant.id)}
                          aria-label={`选择屏幕 ${occupant.name} (${getOsLabel(occupant.os)})`}
                          className="text-left w-full flex-1 flex flex-col justify-between focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-600 rounded"
                        >
                          <div>
                            <div className="flex items-center justify-between gap-1 text-[11px] leading-tight mb-1">
                              <span
                                className={
                                  occupant.isPrimary
                                    ? 'font-semibold text-blue-700 dark:text-blue-400'
                                    : 'text-slate-500 dark:text-slate-400'
                                }
                              >
                                {occupant.isPrimary
                                  ? '主屏幕'
                                  : isLocalMachine
                                    ? '本机屏幕'
                                    : '受控屏幕'}
                                {' · '}
                                <span
                                  className={
                                    occupant.online
                                      ? 'text-emerald-700 dark:text-emerald-400'
                                      : 'text-slate-400 dark:text-slate-500'
                                  }
                                >
                                  {occupant.online ? '在线' : '离线'}
                                </span>
                              </span>
                              {occupant.isPrimary && (
                                <Crown className="w-3.5 h-3.5 text-blue-600 dark:text-blue-400 shrink-0" />
                              )}
                            </div>

                            <div className="flex items-center gap-1.5">
                              {renderOsIcon(occupant.os)}
                              <span className="text-xs font-semibold text-slate-900 dark:text-slate-100 truncate">
                                {occupant.name}
                              </span>
                            </div>
                          </div>

                          <div className="mt-2 pt-2 border-t border-slate-200/70 dark:border-slate-800 space-y-0.5">
                            <div className="text-[11px] text-slate-500 dark:text-slate-400 truncate tabular-nums">
                              {getOsLabel(occupant.os)} · {occupant.refreshRateHz}Hz
                            </div>
                            <div className="text-[11px] font-mono text-slate-500 dark:text-slate-400 truncate tabular-nums">
                              {occupant.ip}
                            </div>
                          </div>
                        </button>

                        {isCursorHere && (
                          <div className="mt-1.5 pt-1 border-t border-blue-200 dark:border-blue-900 flex items-center gap-1 text-[11px] font-medium text-blue-700 dark:text-blue-300">
                            <MousePointer className="w-3 h-3 shrink-0" />
                            <span className="truncate">当前键鼠焦点</span>
                          </div>
                        )}
                      </div>
                    );
                  }

                  return (
                    <button
                      key={`${col}-${row}`}
                      type="button"
                      onDragOver={(e) => e.preventDefault()}
                      onDrop={(e) => {
                        e.preventDefault();
                        if (draggedNodeId) {
                          handleCellAction(draggedNodeId, col, row);
                          setDraggedNodeId(null);
                        }
                      }}
                      onClick={() => {
                        if (selectedNode) {
                          handleCellAction(selectedNode.id, col, row);
                        }
                      }}
                      aria-label={`将选中屏幕移动至网格第 ${row + 1} 行第 ${col + 1} 列`}
                      className="rounded-lg border border-dashed border-slate-200 dark:border-slate-800/90 hover:border-blue-400 dark:hover:border-blue-600 hover:bg-blue-50/30 dark:hover:bg-blue-950/20 min-h-[122px] p-2 flex flex-col items-center justify-center text-center transition-colors group focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-600"
                    >
                      <span className="text-[11px] font-mono text-slate-400 dark:text-slate-600 group-hover:text-blue-600 dark:group-hover:text-blue-400 tabular-nums">
                        ({col + 1}, {row + 1})
                      </span>
                      <span className="text-[11px] text-slate-400 dark:text-slate-600 group-hover:text-blue-600 dark:group-hover:text-blue-400 mt-1 opacity-0 group-hover:opacity-100 transition-opacity">
                        移至此槽位
                      </span>
                    </button>
                  );
                })
              )}
            </div>
          </div>
        </section>

        {/* Right 4 Cols: Selected Screen Configuration & Directional Neighbors */}
        <aside
          aria-label="选中屏幕参数与主屏设置"
          className="xl:col-span-4 bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl divide-y divide-slate-200 dark:divide-slate-800"
        >
          {selectedNode && (
            <>
              <div className="p-5">
                <div className="text-xs text-slate-500 dark:text-slate-400 tabular-nums">
                  选中屏幕节点 · 网格坐标 ({selectedNode.gridPos.col + 1}, {selectedNode.gridPos.row + 1})
                </div>
                <h3 className="text-lg font-semibold text-slate-900 dark:text-slate-100 mt-1">
                  {selectedNode.name}
                </h3>
                <div className="text-xs text-slate-500 dark:text-slate-400 mt-1 tabular-nums">
                  {getOsLabel(selectedNode.os)} · {selectedNode.resolution.width}×{selectedNode.resolution.height} ·{' '}
                  {selectedNode.online
                    ? `延迟 ${selectedNode.latencyMs.toFixed(1)} ms`
                    : '等待设备上线'}
                </div>

                <div className="mt-4 flex flex-wrap gap-2">
                  <button
                    type="button"
                    disabled={selectedNode.isPrimary}
                    onClick={() => {
                      onSetPrimary(selectedNode.id);
                      setTopologyFeedback({
                        tone: 'info',
                        message: `已将「${selectedNode.name}」设为主屏幕（物理键盘与鼠标接入端）。`,
                      });
                    }}
                    className={`flex-1 inline-flex items-center justify-center gap-1.5 px-3.5 py-2 text-xs font-medium rounded-lg transition-colors whitespace-nowrap shrink-0 ${
                      selectedNode.isPrimary
                        ? 'bg-slate-100 dark:bg-slate-800 text-slate-500 dark:text-slate-400 cursor-default'
                        : 'bg-blue-600 text-white hover:bg-blue-700'
                    }`}
                  >
                    <Crown className="w-3.5 h-3.5" />
                    {selectedNode.isPrimary ? '当前已是主屏幕' : '设为主屏幕'}
                  </button>

                  <button
                    type="button"
                    disabled={!selectedNode.online}
                    onClick={() => onJumpCursorToNode(selectedNode.id)}
                    className="inline-flex items-center justify-center gap-1.5 px-3.5 py-2 text-xs font-medium rounded-lg border border-slate-200 dark:border-slate-700 text-slate-700 dark:text-slate-200 hover:bg-slate-50 dark:hover:bg-slate-800 disabled:opacity-40 transition-colors whitespace-nowrap shrink-0"
                  >
                    <MousePointer className="w-3.5 h-3.5" />
                    切入键鼠焦点
                  </button>
                </div>
              </div>

              {/* Up / Down / Left / Right Adjacency Readout & Fine Move */}
              <div className="p-5 space-y-3">
                <div className="flex items-center justify-between">
                  <h4 className="text-sm font-semibold text-slate-900 dark:text-slate-100">
                    上下左右相邻跨屏关系
                  </h4>
                  <span className="text-xs text-slate-500 dark:text-slate-400">方向调整</span>
                </div>

                <div className="space-y-2 text-xs">
                  {(
                    [
                      { dir: '上方边界 (Up)', neighbor: selectedNeighbors.up, dc: 0, dr: -1, icon: ArrowUp },
                      { dir: '下方边界 (Down)', neighbor: selectedNeighbors.down, dc: 0, dr: 1, icon: ArrowDown },
                      { dir: '左侧边界 (Left)', neighbor: selectedNeighbors.left, dc: -1, dr: 0, icon: ArrowLeft },
                      { dir: '右侧边界 (Right)', neighbor: selectedNeighbors.right, dc: 1, dr: 0, icon: ArrowRight },
                    ] as const
                  ).map(({ dir, neighbor, dc, dr, icon: Icon }) => (
                    <div
                      key={dir}
                      className="flex items-center justify-between gap-2 py-1.5 border-b border-slate-100 dark:border-slate-800/70 last:border-b-0"
                    >
                      <div className="min-w-0">
                        <span className="text-slate-500 dark:text-slate-400">{dir}：</span>
                        <span className="font-medium text-slate-900 dark:text-slate-100">
                          {neighbor ? neighbor.name : '无相邻屏幕（边缘阻断）'}
                        </span>
                      </div>
                      <button
                        type="button"
                        onClick={() =>
                          handleCellAction(
                            selectedNode.id,
                            selectedNode.gridPos.col + dc,
                            selectedNode.gridPos.row + dr
                          )
                        }
                        aria-label={`将 ${selectedNode.name} 向${dir.slice(0, 2)}移动一格`}
                        className="inline-flex items-center gap-1 px-2 py-1 rounded border border-slate-200 dark:border-slate-700 text-slate-600 dark:text-slate-300 hover:bg-slate-50 dark:hover:bg-slate-800 whitespace-nowrap shrink-0"
                      >
                        <Icon className="w-3 h-3" />
                        移向此侧
                      </button>
                    </div>
                  ))}
                </div>
              </div>

              {/* High Refresh Rate & Low Latency Parameters */}
              <div className="p-5 space-y-4">
                <h4 className="text-sm font-semibold text-slate-900 dark:text-slate-100">
                  高刷新率与跨屏低延迟参数
                </h4>

                <div className="space-y-1.5">
                  <label
                    htmlFor="node-refresh-select"
                    className="block text-xs text-slate-600 dark:text-slate-400"
                  >
                    当前显示器刷新率档位（光标插值对齐）
                  </label>
                  <select
                    id="node-refresh-select"
                    value={selectedNode.refreshRateHz}
                    onChange={(e) =>
                      onUpdateNodeRefreshRate(
                        selectedNode.id,
                        Number(e.target.value) as DeviceNode['refreshRateHz']
                      )
                    }
                    className="w-full rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-950 pl-3 pr-8 py-2 text-xs font-mono text-slate-900 dark:text-slate-100 tabular-nums focus:outline-none focus:ring-2 focus:ring-blue-600"
                  >
                    <option value={60}>60 Hz · 标准显示器 (16.6ms)</option>
                    <option value={120}>120 Hz · ProMotion 高刷 (8.3ms)</option>
                    <option value={144}>144 Hz · 桌面高刷屏 (6.9ms)</option>
                    <option value={240}>240 Hz · 电竞低延迟 (4.1ms)</option>
                    <option value={360}>360 Hz · OLED 极限帧率 (2.7ms)</option>
                  </select>
                </div>

                <div className="grid grid-cols-2 gap-3">
                  <div>
                    <span className="block text-xs text-slate-600 dark:text-slate-400 mb-1.5">
                      鼠标采样回报率
                    </span>
                    <div className="flex p-0.5 rounded-lg bg-slate-100 dark:bg-slate-800">
                      {([500, 1000] as const).map((hz) => (
                        <button
                          key={hz}
                          type="button"
                          onClick={() => onChangeHidPollingHz(hz)}
                          className={`flex-1 py-1.5 text-xs font-mono rounded-md transition-colors whitespace-nowrap shrink-0 tabular-nums ${
                            hidPollingHz === hz
                              ? 'bg-white dark:bg-slate-900 text-slate-900 dark:text-slate-100 shadow-xs font-medium'
                              : 'text-slate-600 dark:text-slate-400'
                          }`}
                        >
                          {hz}Hz
                        </button>
                      ))}
                    </div>
                  </div>

                  <div>
                    <label
                      htmlFor="edge-delay-select"
                      className="block text-xs text-slate-600 dark:text-slate-400 mb-1.5"
                    >
                      边缘穿屏防误触阻尼
                    </label>
                    <select
                      id="edge-delay-select"
                      value={edgeDelayMs}
                      onChange={(e) => onChangeEdgeDelayMs(Number(e.target.value))}
                      className="w-full rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-950 px-2.5 py-1.5 text-xs font-mono text-slate-900 dark:text-slate-100 tabular-nums"
                    >
                      <option value={0}>0 ms (零阻尼瞬穿)</option>
                      <option value={35}>35 ms (轻度防误触)</option>
                      <option value={80}>80 ms (全屏应用保护)</option>
                    </select>
                  </div>
                </div>
              </div>
            </>
          )}
        </aside>
      </div>

      {/* Live Focus & Hotkey Summary Strip */}
      <section
        aria-label="当前跨屏路由与键盘跟随状态"
        className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl p-5"
      >
        <div className="grid grid-cols-1 md:grid-cols-3 gap-6 items-center">
          <div className="space-y-1">
            <div className="text-xs text-slate-500 dark:text-slate-400">当前鼠标所在屏幕</div>
            <div className="text-sm font-semibold text-slate-900 dark:text-slate-100 flex items-center gap-2">
              <MousePointer className="w-4 h-4 text-blue-600 dark:text-blue-400 shrink-0" />
              <span>{activeFocusNode?.name ?? '本机主屏幕'}</span>
            </div>
            <div className="text-xs font-mono text-slate-500 dark:text-slate-400 tabular-nums">
              光标相对位置 ({Math.round(cursor.x * 100)}%, {Math.round(cursor.y * 100)}%) · 累计跨屏切换 {cursor.transitionCount} 次
            </div>
          </div>

          <div className="space-y-1">
            <div className="text-xs text-slate-500 dark:text-slate-400">键盘输入实时跟随目标</div>
            <div className="text-sm font-semibold text-slate-900 dark:text-slate-100 flex items-center gap-2">
              <Keyboard className="w-4 h-4 text-blue-600 dark:text-blue-400 shrink-0" />
              <span>
                {activeFocusNode?.name} ({activeFocusNode ? getOsLabel(activeFocusNode.os) : ''})
              </span>
            </div>
            <div className="text-xs text-slate-500 dark:text-slate-400 tabular-nums">
              QUIC TLS 1.3 加密传输 · 监听 UDP {systemStatus.quicPort} · 在线互联 {systemStatus.connectedPeerCount} 台
            </div>
          </div>

          <div className="space-y-1 md:text-right">
            <div className="text-xs text-slate-500 dark:text-slate-400">全局紧急收回键鼠热键</div>
            <div className="text-sm font-mono font-semibold text-blue-700 dark:text-blue-400">
              {releaseHotkey}
            </div>
            <div className="text-xs text-slate-500 dark:text-slate-400">
              任何时候按下该组合键立即解除远端接管并回到本机屏幕
            </div>
          </div>
        </div>
      </section>
    </div>
  );
};
