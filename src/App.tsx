/**
 * @license
 * SPDX-License-Identifier: Apache-2.0
 */

import React, { useCallback, useEffect, useState } from 'react';
import {
  LayoutGrid,
  ShieldCheck,
  Keyboard,
  Clipboard,
  Settings2,
  Sun,
  Moon,
  MonitorSmartphone,
} from 'lucide-react';
import {
  DEFAULT_QUIC_PORT,
  DISCOVERY_UDP_PORT,
  type ClipboardPolicy,
  type DeviceNode,
  type DiscoveredPeer,
  type EngineTuning,
  type KeyMappingRule,
  type LocalSystemStatus,
  type TransferRecord,
  type VirtualCursorState,
} from './utils/kvmEngine';
import {
  INITIAL_DISCOVERED_PEERS,
  INITIAL_KEY_RULES,
  INITIAL_NODES,
  INITIAL_TRANSFERS,
} from './data/seedData';
import {
  DEFAULT_CLIPBOARD_POLICY,
  DEFAULT_TUNING,
  type DaemonSnapshot,
  invokeDaemonCommand,
  isTauriNativeRuntime,
} from './services/tauriBridge';
import { TopologyWorkspace } from './components/TopologyWorkspace';
import { PairingSecurityView } from './components/PairingSecurityView';
import { KeyMappingView } from './components/KeyMappingView';
import { ClipboardTransferView } from './components/ClipboardTransferView';
import { SystemSettingsView } from './components/SystemSettingsView';

type ActiveTab = 'topology' | 'pairing' | 'keymap' | 'clipboard' | 'settings';

const NAV_ITEMS: {
  id: ActiveTab;
  label: string;
  icon: React.ComponentType<{ className?: string }>;
}[] = [
  { id: 'topology', label: '屏幕拓扑与控制', icon: LayoutGrid },
  { id: 'pairing', label: '设备发现与配对', icon: ShieldCheck },
  { id: 'keymap', label: '跨系统按键映射', icon: Keyboard },
  { id: 'clipboard', label: '剪贴板与文件传输', icon: Clipboard },
  { id: 'settings', label: '系统与托盘设置', icon: Settings2 },
];

export default function App() {
  const [activeTab, setActiveTab] = useState<ActiveTab>('topology');
  const [darkMode, setDarkMode] = useState<boolean>(false);

  const [nodes, setNodes] = useState<DeviceNode[]>(INITIAL_NODES);
  const [discoveredPeers, setDiscoveredPeers] = useState<DiscoveredPeer[]>(
    INITIAL_DISCOVERED_PEERS
  );
  const [selectedNodeId, setSelectedNodeId] = useState<string>(
    INITIAL_NODES[0].id
  );
  const [cursor, setCursor] = useState<VirtualCursorState>({
    activeNodeId: INITIAL_NODES[0].id,
    x: 0.5,
    y: 0.5,
    transitionCount: 0,
  });

  const [tuning, setTuning] = useState<EngineTuning>(DEFAULT_TUNING);
  const [clipboardPolicy, setClipboardPolicy] = useState<ClipboardPolicy>(
    DEFAULT_CLIPBOARD_POLICY
  );

  const [localEphemeralPin, setLocalEphemeralPin] = useState<string>('482-915');
  const [activeDriverName, setActiveDriverName] = useState<string>(
    'macOS CoreGraphics CGEventTap + CGEventPost'
  );
  const [systemStatus, setSystemStatus] = useState<LocalSystemStatus>({
    localNodeId: INITIAL_NODES[0].id,
    localOs: INITIAL_NODES[0].os,
    localIp: INITIAL_NODES[0].ip,
    quicPort: DEFAULT_QUIC_PORT,
    discoveryPort: DISCOVERY_UDP_PORT,
    fingerprint: INITIAL_NODES[0].fingerprint,
    inputPermissionGranted: true,
    connectedPeerCount: INITIAL_NODES.filter((n) => !n.isPrimary && n.online).length,
  });
  const [keyRules, setKeyRules] = useState<KeyMappingRule[]>(INITIAL_KEY_RULES);
  const [transfers, setTransfers] = useState<TransferRecord[]>(INITIAL_TRANSFERS);

  const applySnapshot = useCallback((snap?: DaemonSnapshot) => {
    if (!snap) return;
    setNodes(snap.nodes);
    setSelectedNodeId((prev) =>
      snap.nodes.some((n) => n.id === prev) ? prev : (snap.nodes[0]?.id ?? prev)
    );
    setDiscoveredPeers(snap.discoveredPeers);
    setCursor(snap.cursor);
    setTuning(snap.tuning);
    setClipboardPolicy(snap.clipboardPolicy);
    setKeyRules(snap.keyRules);
    setTransfers(snap.transfers);
    setLocalEphemeralPin(snap.localEphemeralPin);
    setActiveDriverName(snap.activeDriverName);
    setSystemStatus(snap.systemStatus);
  }, []);

  // Hydrate persistent state from Tauri v2 IPC / Express daemon bridge on startup,
  // and poll periodically in native desktop mode so LAN discovery & tray actions stay synced
  useEffect(() => {
    let active = true;
    const syncOnce = async () => {
      const res = await invokeDaemonCommand('get_daemon_snapshot');
      if (active && res.ok && res.snapshot) {
        applySnapshot(res.snapshot);
      }
    };
    syncOnce();

    if (!isTauriNativeRuntime()) {
      return () => {
        active = false;
      };
    }

    const timer = window.setInterval(syncOnce, 2500);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [applySnapshot]);

  useEffect(() => {
    const root = document.documentElement;
    if (darkMode) {
      root.classList.add('dark');
    } else {
      root.classList.remove('dark');
    }
  }, [darkMode]);

  const handleEmergencyReturnPrimary = useCallback(async () => {
    const res = await invokeDaemonCommand('emergency_return_primary');
    if (res.ok) applySnapshot(res.snapshot);
  }, [applySnapshot]);

  // Global emergency focus-reclaim shortcut inside window: Ctrl + Alt + Shift + Escape
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.altKey && e.shiftKey && e.key === 'Escape') {
        e.preventDefault();
        handleEmergencyReturnPrimary();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [handleEmergencyReturnPrimary]);

  const handleSetPrimary = async (primaryId: string) => {
    const res = await invokeDaemonCommand('set_primary_node', { primaryId });
    if (res.ok) applySnapshot(res.snapshot);
  };

  const handleMoveNode = async (nodeId: string, col: number, row: number) => {
    const res = await invokeDaemonCommand('move_node_cell', { nodeId, col, row });
    if (res.ok) {
      applySnapshot(res.snapshot);
    }
    return {
      ok: res.ok,
      error: res.error,
      swappedWith: res.swappedWith,
    };
  };

  const handleUpdateRefreshRate = async (
    nodeId: string,
    hz: DeviceNode['refreshRateHz']
  ) => {
    const res = await invokeDaemonCommand('update_node_refresh_rate', {
      nodeId,
      hz,
    });
    if (res.ok) applySnapshot(res.snapshot);
  };

  const handleUpdateTuning = async (patch: Partial<EngineTuning>) => {
    setTuning((prev) => ({ ...prev, ...patch }));
    const res = await invokeDaemonCommand('update_engine_tuning', {
      tuning: patch,
    });
    if (res.ok) applySnapshot(res.snapshot);
  };

  const handleJumpCursorToNode = async (nodeId: string) => {
    const res = await invokeDaemonCommand('jump_cursor_to_node', { nodeId });
    if (res.ok) applySnapshot(res.snapshot);
  };

  const handleRequestInputPermission = async () => {
    const res = await invokeDaemonCommand('request_input_permission');
    if (res.ok) applySnapshot(res.snapshot);
  };

  const handleScanLanPeers = async () => {
    const res = await invokeDaemonCommand('scan_lan_peers');
    if (res.ok) applySnapshot(res.snapshot);
  };

  const handlePairPeer = async (
    peer: DiscoveredPeer,
    pinInput: string,
    autoPair: boolean
  ) => {
    const res = await invokeDaemonCommand<{
      ok: boolean;
      snapshot?: DaemonSnapshot;
      addedNode?: DeviceNode;
      error?: string;
    }>('pair_peer_spake2', {
      peerId: peer.id,
      pinInput,
      autoPair,
    });
    if (res.ok) {
      applySnapshot(res.snapshot);
      if (res.addedNode) {
        setSelectedNodeId(res.addedNode.id);
      }
    }
    return { ok: res.ok, error: res.error };
  };

  const handlePairByIp = async (
    ip: string,
    port: number,
    pinInput: string,
    autoPair: boolean
  ) => {
    const res = await invokeDaemonCommand<{
      ok: boolean;
      snapshot?: DaemonSnapshot;
      addedNode?: DeviceNode;
      error?: string;
    }>('pair_by_ip_spake2', {
      ip,
      port,
      pinInput,
      autoPair,
    });
    if (res.ok) {
      applySnapshot(res.snapshot);
      if (res.addedNode) {
        setSelectedNodeId(res.addedNode.id);
      }
    }
    return { ok: res.ok, error: res.error };
  };

  const handleToggleAutoPair = async (nodeId: string) => {
    const res = await invokeDaemonCommand('toggle_auto_pair', { nodeId });
    if (res.ok) applySnapshot(res.snapshot);
  };

  const handleRemoveNode = async (nodeId: string) => {
    const res = await invokeDaemonCommand('remove_paired_node', { nodeId });
    if (res.ok && res.snapshot) {
      applySnapshot(res.snapshot);
      if (selectedNodeId === nodeId && res.snapshot.nodes[0]) {
        setSelectedNodeId(res.snapshot.nodes[0].id);
      }
    }
  };

  const handleToggleKeyRule = async (ruleId: string) => {
    const res = await invokeDaemonCommand('toggle_key_rule', { ruleId });
    if (res.ok) applySnapshot(res.snapshot);
  };

  const handleDeleteKeyRule = async (ruleId: string) => {
    const res = await invokeDaemonCommand('delete_key_rule', { ruleId });
    if (res.ok) applySnapshot(res.snapshot);
  };

  const handleAddKeyRule = async (rule: Omit<KeyMappingRule, 'id'>) => {
    const res = await invokeDaemonCommand('add_key_rule', { rule });
    if (res.ok) applySnapshot(res.snapshot);
  };

  const handleUpdateClipboardPolicy = async (
    patch: Partial<ClipboardPolicy>
  ) => {
    setClipboardPolicy((prev) => ({ ...prev, ...patch }));
    const res = await invokeDaemonCommand('update_clipboard_policy', {
      policy: patch,
    });
    if (res.ok) applySnapshot(res.snapshot);
  };

  const handleBroadcastClipboard = async (text: string) => {
    const res = await invokeDaemonCommand('broadcast_clipboard_text', { text });
    if (res.ok) applySnapshot(res.snapshot);
    return { ok: res.ok, error: res.error };
  };

  const handleSendFilePayload = async (
    fileName: string,
    targetNodeId: string,
    base64Data: string
  ) => {
    const res = await invokeDaemonCommand('send_file_payload', {
      fileName,
      targetNodeId,
      base64Data,
    });
    if (res.ok) applySnapshot(res.snapshot);
    return { ok: res.ok, error: res.error };
  };

  const handleOpenReceiveDir = async () => {
    const res = await invokeDaemonCommand('open_receive_dir');
    if (res.ok) applySnapshot(res.snapshot);
  };

  const handleClearTransfers = async () => {
    const res = await invokeDaemonCommand('clear_transfer_records');
    if (res.ok) applySnapshot(res.snapshot);
  };

  const currentNavLabel =
    NAV_ITEMS.find((item) => item.id === activeTab)?.label ?? '屏幕拓扑与控制';
  const primaryNode = nodes.find((n) => n.isPrimary) ?? nodes[0];
  const activeFocusNode =
    nodes.find((n) => n.id === cursor?.activeNodeId) ?? primaryNode;

  return (
    <div className="min-h-screen bg-slate-50 dark:bg-slate-950 text-slate-900 dark:text-slate-100 flex flex-col md:flex-row">
      {/* Left Desktop Sidebar Navigation (256px) */}
      <aside className="w-full md:w-64 shrink-0 bg-white dark:bg-slate-900 border-b md:border-b-0 md:border-r border-slate-200 dark:border-slate-800 flex flex-col justify-between">
        <div>
          <div className="px-5 py-4 border-b border-slate-200 dark:border-slate-800 flex items-center justify-between">
            <div className="flex items-center gap-2.5 min-w-0">
              <div className="w-8 h-8 rounded-lg bg-blue-600 text-white flex items-center justify-center shrink-0">
                <MonitorSmartphone className="w-4 h-4" />
              </div>
              <span className="text-base font-bold tracking-tight text-slate-900 dark:text-slate-100 whitespace-nowrap shrink-0">
                SpanKVM
              </span>
            </div>
            <span className="text-xs text-slate-500 dark:text-slate-400 md:hidden tabular-nums">
              {nodes.length}/9 台
            </span>
          </div>

          <nav
            aria-label="主功能模块导航"
            className="p-3 flex md:flex-col gap-1 overflow-x-auto md:overflow-visible"
          >
            {NAV_ITEMS.map((item) => {
              const Icon = item.icon;
              const isCurrent = activeTab === item.id;
              return (
                <button
                  key={item.id}
                  type="button"
                  onClick={() => setActiveTab(item.id)}
                  className={`flex items-center gap-2.5 px-3 py-2.5 rounded-lg text-xs font-medium transition-colors whitespace-nowrap shrink-0 md:w-full text-left ${
                    isCurrent
                      ? 'bg-blue-50 dark:bg-blue-950/50 text-blue-700 dark:text-blue-300'
                      : 'text-slate-600 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-800/70 hover:text-slate-900 dark:hover:text-slate-100'
                  }`}
                >
                  <Icon className="w-4 h-4 shrink-0" />
                  <span>{item.label}</span>
                </button>
              );
            })}
          </nav>
        </div>

        {/* Local Host & Active Focus Summary in Sidebar Footer */}
        <div className="hidden md:block p-4 m-3 rounded-lg bg-slate-50 dark:bg-slate-950 border border-slate-200/80 dark:border-slate-800 space-y-2 text-xs">
          <div className="font-mono text-slate-500 dark:text-slate-400 truncate tabular-nums">
            本机：{systemStatus.localIp}:{systemStatus.quicPort}
          </div>
          <div className="font-semibold text-slate-900 dark:text-slate-100 truncate">
            当前焦点：{activeFocusNode?.name}
          </div>
          <div className="text-slate-500 dark:text-slate-400 pt-1 border-t border-slate-200/70 dark:border-slate-800 truncate tabular-nums">
            主屏幕：{primaryNode?.name}
          </div>
          <div className="text-slate-500 dark:text-slate-400 truncate tabular-nums">
            已配置节点：{nodes.length} / 9 台 · 在线 {systemStatus.connectedPeerCount + 1} 台
          </div>
        </div>
      </aside>

      {/* Right Main Workspace Viewport */}
      <div className="flex-1 min-w-0 flex flex-col">
        <header className="bg-white dark:bg-slate-900 border-b border-slate-200 dark:border-slate-800 px-6 py-3.5 flex items-center justify-between gap-8">
          <div className="text-xs sm:text-sm text-slate-500 dark:text-slate-400 truncate">
            <span>SpanKVM 跨系统键鼠共享</span>
            <span className="mx-2 text-slate-300 dark:text-slate-700">/</span>
            <span className="font-medium text-slate-900 dark:text-slate-100">
              {currentNavLabel}
            </span>
          </div>

          <div className="flex items-center gap-2.5 shrink-0">
            <button
              type="button"
              onClick={() =>
                handleUpdateTuning({ daemonActive: !tuning.daemonActive })
              }
              className={`px-3 py-1.5 text-xs font-medium rounded-lg border transition-colors whitespace-nowrap shrink-0 ${
                tuning.daemonActive
                  ? 'border-blue-200 dark:border-blue-800 bg-blue-50 dark:bg-blue-950/40 text-blue-700 dark:text-blue-300'
                  : 'border-amber-300 dark:border-amber-800 bg-amber-50 dark:bg-amber-950/40 text-amber-800 dark:text-amber-300'
              }`}
            >
              {tuning.daemonActive ? '跨屏共享：运行中' : '跨屏共享：已暂停'}
            </button>

            <button
              type="button"
              onClick={() => setDarkMode((v) => !v)}
              aria-label={darkMode ? '切换至浅色界面' : '切换至深色界面'}
              className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg border border-slate-200 dark:border-slate-700 text-slate-700 dark:text-slate-200 hover:bg-slate-50 dark:hover:bg-slate-800 transition-colors whitespace-nowrap shrink-0"
            >
              {darkMode ? (
                <Sun className="w-3.5 h-3.5" />
              ) : (
                <Moon className="w-3.5 h-3.5" />
              )}
              <span>{darkMode ? '浅色外观' : '深色外观'}</span>
            </button>
          </div>
        </header>

        <main className="flex-1 p-5 sm:p-8 max-w-[1360px] w-full mx-auto">
          {activeTab === 'topology' && (
            <TopologyWorkspace
              nodes={nodes}
              selectedNodeId={selectedNodeId}
              onSelectNode={setSelectedNodeId}
              onSetPrimary={handleSetPrimary}
              onMoveNode={handleMoveNode}
              onUpdateNodeRefreshRate={handleUpdateRefreshRate}
              cursor={cursor}
              onJumpCursorToNode={handleJumpCursorToNode}
              onEmergencyReturnPrimary={handleEmergencyReturnPrimary}
              daemonActive={tuning.daemonActive}
              onToggleDaemonActive={() =>
                handleUpdateTuning({ daemonActive: !tuning.daemonActive })
              }
              onNavigateToPairing={() => setActiveTab('pairing')}
              hidPollingHz={tuning.hidPollingHz}
              onChangeHidPollingHz={(hz) =>
                handleUpdateTuning({ hidPollingHz: hz })
              }
              edgeDelayMs={tuning.edgeDelayMs}
              onChangeEdgeDelayMs={(ms) =>
                handleUpdateTuning({ edgeDelayMs: ms })
              }
              releaseHotkey={tuning.releaseHotkey}
              systemStatus={systemStatus}
              onRequestInputPermission={handleRequestInputPermission}
            />
          )}

          {activeTab === 'pairing' && (
            <PairingSecurityView
              nodes={nodes}
              discoveredPeers={discoveredPeers}
              localEphemeralPin={localEphemeralPin}
              localIp={systemStatus.localIp}
              quicPort={systemStatus.quicPort}
              localNodeId={systemStatus.localNodeId}
              onRefreshLocalPin={async () => {
                const res = await invokeDaemonCommand('rotate_ephemeral_pin');
                if (res.ok) applySnapshot(res.snapshot);
              }}
              onScanLanPeers={handleScanLanPeers}
              onPairPeer={handlePairPeer}
              onPairByIp={handlePairByIp}
              onToggleAutoPair={handleToggleAutoPair}
              onRemoveNode={handleRemoveNode}
            />
          )}

          {activeTab === 'keymap' && (
            <KeyMappingView
              rules={keyRules}
              nodes={nodes}
              activeFocusNodeId={cursor?.activeNodeId ?? primaryNode?.id ?? ''}
              onToggleRule={handleToggleKeyRule}
              onDeleteRule={handleDeleteKeyRule}
              onAddRule={handleAddKeyRule}
            />
          )}

          {activeTab === 'clipboard' && (
            <ClipboardTransferView
              nodes={nodes}
              policy={clipboardPolicy}
              onUpdatePolicy={handleUpdateClipboardPolicy}
              transfers={transfers}
              onBroadcastClipboard={handleBroadcastClipboard}
              onSendFilePayload={handleSendFilePayload}
              onOpenReceiveDir={handleOpenReceiveDir}
              onClearTransfers={handleClearTransfers}
            />
          )}

          {activeTab === 'settings' && (
            <SystemSettingsView
              tuning={tuning}
              systemStatus={systemStatus}
              activeDriverName={activeDriverName}
              onUpdateTuning={handleUpdateTuning}
              onRequestInputPermission={handleRequestInputPermission}
            />
          )}
        </main>
      </div>
    </div>
  );
}
