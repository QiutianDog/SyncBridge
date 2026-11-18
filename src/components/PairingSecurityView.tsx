import React, { useState } from 'react';
import {
  ShieldCheck,
  KeyRound,
  RefreshCw,
  Wifi,
  Trash2,
  Search,
  Lock,
  Check,
  Copy,
  Radio,
} from 'lucide-react';
import {
  DEFAULT_QUIC_PORT,
  DeviceNode,
  DiscoveredPeer,
  MAX_DEVICES,
  getOsLabel,
  validateLanIpv4,
  validatePairingPin,
} from '../utils/kvmEngine';

interface PairingSecurityViewProps {
  nodes: DeviceNode[];
  discoveredPeers: DiscoveredPeer[];
  localEphemeralPin: string;
  localIp: string;
  quicPort: number;
  localNodeId: string;
  onRefreshLocalPin: () => void;
  onScanLanPeers: () => Promise<void>;
  onPairPeer: (
    peer: DiscoveredPeer,
    pinInput: string,
    autoPairNextTime: boolean
  ) => Promise<{ ok: boolean; error?: string }>;
  onPairByIp: (
    ip: string,
    port: number,
    pinInput: string,
    autoPairNextTime: boolean
  ) => Promise<{ ok: boolean; error?: string }>;
  onToggleAutoPair: (nodeId: string) => void;
  onRemoveNode: (nodeId: string) => void;
}

export const PairingSecurityView: React.FC<PairingSecurityViewProps> = ({
  nodes,
  discoveredPeers,
  localEphemeralPin,
  localIp,
  quicPort,
  localNodeId,
  onRefreshLocalPin,
  onScanLanPeers,
  onPairPeer,
  onPairByIp,
  onToggleAutoPair,
  onRemoveNode,
}) => {
  const [pairMode, setPairMode] = useState<'discovered' | 'manual-ip'>('discovered');
  const [selectedPeerId, setSelectedPeerId] = useState<string>(
    discoveredPeers[0]?.id ?? ''
  );
  const [manualIpInput, setManualIpInput] = useState<string>('');
  const [manualPortInput, setManualPortInput] = useState<string>(
    String(DEFAULT_QUIC_PORT)
  );
  const [pinInput, setPinInput] = useState<string>('');
  const [rememberAutoPair, setRememberAutoPair] = useState<boolean>(true);
  const [isScanning, setIsScanning] = useState<boolean>(false);
  const [isPairing, setIsPairing] = useState<boolean>(false);
  const [copiedPin, setCopiedPin] = useState<boolean>(false);
  const [formFeedback, setFormFeedback] = useState<{
    tone: 'success' | 'error';
    text: string;
  } | null>(null);
  const [searchQuery, setSearchQuery] = useState<string>('');

  const activePeer =
    discoveredPeers.find((p) => p.id === selectedPeerId) ?? discoveredPeers[0];

  const handleCopyLocalPin = async () => {
    try {
      await navigator.clipboard.writeText(localEphemeralPin);
      setCopiedPin(true);
      setTimeout(() => setCopiedPin(false), 1800);
    } catch {
      // Ignore clipboard write failure in restricted contexts
    }
  };

  const handleTriggerScan = async () => {
    setIsScanning(true);
    await onScanLanPeers();
    setTimeout(() => setIsScanning(false), 350);
  };

  const handlePairSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    const pinCheck = validatePairingPin(pinInput);
    if (!pinCheck.valid) {
      setFormFeedback({
        tone: 'error',
        text: pinCheck.error ?? '请输入有效的 6 位数字临时配对码。',
      });
      return;
    }

    setIsPairing(true);
    try {
      if (pairMode === 'manual-ip') {
        const ipCheck = validateLanIpv4(manualIpInput);
        if (!ipCheck.valid) {
          setFormFeedback({
            tone: 'error',
            text: ipCheck.error ?? '请输入有效的局域网 IPv4 地址。',
          });
          return;
        }
        const parsedPort = ipCheck.port ?? (Number(manualPortInput) || DEFAULT_QUIC_PORT);
        const res = await onPairByIp(
          ipCheck.ip,
          parsedPort,
          pinCheck.normalized,
          rememberAutoPair
        );
        if (!res.ok) {
          setFormFeedback({ tone: 'error', text: res.error ?? '局域网直连配对失败。' });
          return;
        }
        setPinInput('');
        setManualIpInput('');
        setFormFeedback({
          tone: 'success',
          text: `已通过 SPAKE2 加密握手与 ${ipCheck.ip}:${parsedPort} 完成配对${
            rememberAutoPair ? '，并已记录公钥指纹支持下次自动重连' : ''
          }。`,
        });
        return;
      }

      if (!activePeer) {
        setFormFeedback({
          tone: 'error',
          text: '当前未扫描到待配对设备，可点击「刷新扫描」或切换至「手动输入 IP 配对」。',
        });
        return;
      }

      const res = await onPairPeer(activePeer, pinCheck.normalized, rememberAutoPair);
      if (!res.ok) {
        setFormFeedback({ tone: 'error', text: res.error ?? '配对失败。' });
        return;
      }

      setPinInput('');
      setFormFeedback({
        tone: 'success',
        text: `已通过 SPAKE2 临时密钥完成与「${activePeer.name}」的双向证书绑定${
          rememberAutoPair ? '，下次同一局域网上线将自动加密重连' : ''
        }。`,
      });
    } finally {
      setIsPairing(false);
    }
  };

  const filteredTrustedNodes = nodes.filter((node) => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return true;
    return (
      node.name.toLowerCase().includes(q) ||
      node.ip.toLowerCase().includes(q) ||
      getOsLabel(node.os).toLowerCase().includes(q)
    );
  });

  return (
    <div className="space-y-6">
      <div className="pb-5 border-b border-slate-200 dark:border-slate-800">
        <h1 className="text-xl font-semibold tracking-tight text-slate-900 dark:text-slate-100">
          局域网设备发现、临时密钥配对与加密互信
        </h1>
        <p className="text-sm text-slate-600 dark:text-slate-400 mt-1">
          严格限定局域网内直连通信。首次配对使用 6 位一次性临时码完成 SPAKE2 零知识密钥交换并绑定 Ed25519 公钥指纹，后续上线自动恢复 QUIC + TLS 1.3 加密连接。
        </p>
      </div>

      {/* Top Row: Local Ephemeral Key Card + Remote Peer Pairing Form */}
      <div className="grid grid-cols-1 lg:grid-cols-12 gap-6 items-stretch">
        {/* Left 5 Cols: This Machine's Temporary Pairing Key */}
        <section
          aria-label="本机局域网临时配对密钥"
          className="lg:col-span-5 bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl p-6 flex flex-col justify-between"
        >
          <div>
            <div className="flex items-center justify-between gap-2">
              <span className="text-xs font-medium text-slate-500 dark:text-slate-400">
                本机被动配对凭证 · 配对成功后自动轮换
              </span>
              <Lock className="w-4 h-4 text-blue-600 dark:text-blue-400 shrink-0" />
            </div>

            <div className="mt-4 py-5 px-4 rounded-lg bg-slate-50 dark:bg-slate-950 border border-slate-200/80 dark:border-slate-800 text-center">
              <div className="text-xs text-slate-500 dark:text-slate-400 mb-1">
                其他局域网电脑连接本机时输入此 6 位临时码
              </div>
              <div
                data-testid="local-ephemeral-pin"
                className="text-3xl font-semibold font-mono tracking-wider text-slate-900 dark:text-slate-100 tabular-nums"
              >
                {localEphemeralPin}
              </div>
            </div>

            <dl className="mt-4 space-y-2 text-xs text-slate-600 dark:text-slate-400">
              <div className="flex justify-between gap-4">
                <dt>本机局域网连接地址</dt>
                <dd className="font-mono text-slate-900 dark:text-slate-200 tabular-nums">
                  {localIp}:{quicPort}
                </dd>
              </div>
              <div className="flex justify-between gap-4">
                <dt>传输加密协议</dt>
                <dd className="font-mono text-slate-900 dark:text-slate-200">
                  QUIC + TLS 1.3 · SPAKE2
                </dd>
              </div>
              <div className="flex justify-between gap-4">
                <dt>已配对席位</dt>
                <dd className="font-mono text-slate-900 dark:text-slate-200 tabular-nums">
                  {nodes.length} / {MAX_DEVICES} 台电脑
                </dd>
              </div>
            </dl>
          </div>

          <div className="mt-5 pt-4 border-t border-slate-100 dark:border-slate-800 flex items-center justify-end gap-2">
            <button
              type="button"
              onClick={handleCopyLocalPin}
              className="inline-flex items-center gap-1.5 px-3 py-2 text-xs font-medium rounded-lg border border-slate-200 dark:border-slate-700 text-slate-700 dark:text-slate-200 hover:bg-slate-50 dark:hover:bg-slate-800 transition-colors whitespace-nowrap shrink-0"
            >
              <Copy className="w-3.5 h-3.5" />
              <span>{copiedPin ? '已复制配对码' : '复制配对码'}</span>
            </button>
            <button
              type="button"
              onClick={onRefreshLocalPin}
              className="inline-flex items-center gap-1.5 px-3.5 py-2 text-xs font-medium rounded-lg border border-slate-200 dark:border-slate-700 text-slate-700 dark:text-slate-200 hover:bg-slate-50 dark:hover:bg-slate-800 transition-colors whitespace-nowrap shrink-0"
            >
              <RefreshCw className="w-3.5 h-3.5" />
              <span>更换临时码</span>
            </button>
          </div>
        </section>

        {/* Right 7 Cols: Pair New Discovered or Manual IP LAN Computer */}
        <section
          aria-label="连接并配对局域网设备"
          className="lg:col-span-7 bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl p-6 flex flex-col justify-between"
        >
          <form onSubmit={handlePairSubmit} className="space-y-4">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div>
                <h2 className="text-base font-semibold text-slate-900 dark:text-slate-100">
                  主动连接局域网内的其他电脑
                </h2>
                <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5">
                  支持通过 UDP 局域网广播自动发现设备，或在跨子网 / AP 隔离环境下直接输入对方 IP 配对。
                </p>
              </div>

              <div
                role="group"
                aria-label="选择配对连接方式"
                className="flex items-center gap-1 p-1 rounded-lg bg-slate-100 dark:bg-slate-800 shrink-0"
              >
                <button
                  type="button"
                  onClick={() => setPairMode('discovered')}
                  className={`px-3 py-1.5 text-xs font-medium rounded-md transition-colors whitespace-nowrap shrink-0 ${
                    pairMode === 'discovered'
                      ? 'bg-white dark:bg-slate-900 text-slate-900 dark:text-slate-100 shadow-xs'
                      : 'text-slate-600 dark:text-slate-400 hover:text-slate-900 dark:hover:text-slate-200'
                  }`}
                >
                  自动发现的设备
                </button>
                <button
                  type="button"
                  onClick={() => setPairMode('manual-ip')}
                  className={`px-3 py-1.5 text-xs font-medium rounded-md transition-colors whitespace-nowrap shrink-0 ${
                    pairMode === 'manual-ip'
                      ? 'bg-white dark:bg-slate-900 text-slate-900 dark:text-slate-100 shadow-xs'
                      : 'text-slate-600 dark:text-slate-400 hover:text-slate-900 dark:hover:text-slate-200'
                  }`}
                >
                  手动输入 IP 配对
                </button>
              </div>
            </div>

            {formFeedback && (
              <div
                role="alert"
                className={`px-3.5 py-2.5 rounded-lg border text-xs ${
                  formFeedback.tone === 'error'
                    ? 'bg-red-50 dark:bg-red-950/30 border-red-200 dark:border-red-900 text-red-700 dark:text-red-300'
                    : 'bg-emerald-50 dark:bg-emerald-950/30 border-emerald-200 dark:border-emerald-900 text-emerald-800 dark:text-emerald-300'
                }`}
              >
                {formFeedback.text}
              </div>
            )}

            {pairMode === 'discovered' ? (
              <div className="grid grid-cols-1 md:grid-cols-12 gap-4">
                <div className="md:col-span-7">
                  <div className="flex items-center justify-between mb-1.5">
                    <label
                      htmlFor="discovered-peer-select"
                      className="block text-xs font-medium text-slate-700 dark:text-slate-300"
                    >
                      在线待配对电脑 ({discoveredPeers.length} 台)
                    </label>
                    <button
                      type="button"
                      onClick={handleTriggerScan}
                      className="inline-flex items-center gap-1 text-xs text-blue-600 dark:text-blue-400 hover:underline whitespace-nowrap shrink-0"
                    >
                      <Radio className={`w-3 h-3 ${isScanning ? 'animate-spin' : ''}`} />
                      <span>刷新扫描</span>
                    </button>
                  </div>
                  <select
                    id="discovered-peer-select"
                    value={activePeer?.id ?? ''}
                    onChange={(e) => setSelectedPeerId(e.target.value)}
                    disabled={discoveredPeers.length === 0}
                    className="w-full rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-950 pl-3 pr-8 py-2 text-xs font-medium text-slate-900 dark:text-slate-100 focus:outline-none focus:ring-2 focus:ring-blue-600"
                  >
                    {discoveredPeers.length === 0 ? (
                      <option value="">暂未发现未配对主机（可切换至手动输入 IP）</option>
                    ) : (
                      discoveredPeers.map((peer) => (
                        <option key={peer.id} value={peer.id}>
                          {peer.name} ({peer.ip} · {getOsLabel(peer.os)})
                        </option>
                      ))
                    )}
                  </select>
                </div>

                <div className="md:col-span-5">
                  <label
                    htmlFor="pairing-pin-input"
                    className="block text-xs font-medium text-slate-700 dark:text-slate-300 mb-1.5"
                  >
                    对方屏幕显示的 6 位临时配对码
                  </label>
                  <input
                    id="pairing-pin-input"
                    type="text"
                    inputMode="numeric"
                    placeholder="输入 6 位数字配对码"
                    value={pinInput}
                    onChange={(e) => setPinInput(e.target.value)}
                    className="w-full rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-950 px-3 py-2 text-xs font-mono text-slate-900 dark:text-slate-100 tabular-nums focus:outline-none focus:ring-2 focus:ring-blue-600"
                  />
                </div>
              </div>
            ) : (
              <div className="grid grid-cols-1 md:grid-cols-12 gap-3">
                <div className="md:col-span-5">
                  <label
                    htmlFor="manual-ip-input"
                    className="block text-xs font-medium text-slate-700 dark:text-slate-300 mb-1.5"
                  >
                    目标电脑局域网 IPv4 地址
                  </label>
                  <input
                    id="manual-ip-input"
                    type="text"
                    placeholder="例如：192.168.50.25"
                    value={manualIpInput}
                    onChange={(e) => setManualIpInput(e.target.value)}
                    className="w-full rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-950 px-3 py-2 text-xs font-mono text-slate-900 dark:text-slate-100 tabular-nums focus:outline-none focus:ring-2 focus:ring-blue-600"
                  />
                </div>

                <div className="md:col-span-3">
                  <label
                    htmlFor="manual-port-input"
                    className="block text-xs font-medium text-slate-700 dark:text-slate-300 mb-1.5"
                  >
                    QUIC 端口
                  </label>
                  <input
                    id="manual-port-input"
                    type="number"
                    min={1}
                    max={65535}
                    value={manualPortInput}
                    onChange={(e) => setManualPortInput(e.target.value)}
                    className="w-full rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-950 px-3 py-2 text-xs font-mono text-slate-900 dark:text-slate-100 tabular-nums focus:outline-none focus:ring-2 focus:ring-blue-600"
                  />
                </div>

                <div className="md:col-span-4">
                  <label
                    htmlFor="manual-pin-input"
                    className="block text-xs font-medium text-slate-700 dark:text-slate-300 mb-1.5"
                  >
                    对方 6 位临时配对码
                  </label>
                  <input
                    id="manual-pin-input"
                    type="text"
                    inputMode="numeric"
                    placeholder="例如：482-915"
                    value={pinInput}
                    onChange={(e) => setPinInput(e.target.value)}
                    className="w-full rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-950 px-3 py-2 text-xs font-mono text-slate-900 dark:text-slate-100 tabular-nums focus:outline-none focus:ring-2 focus:ring-blue-600"
                  />
                </div>
              </div>
            )}

            <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 pt-2">
              <label className="inline-flex items-center gap-2.5 text-xs text-slate-700 dark:text-slate-300 cursor-pointer">
                <input
                  type="checkbox"
                  checked={rememberAutoPair}
                  onChange={(e) => setRememberAutoPair(e.target.checked)}
                  className="rounded border-slate-300 text-blue-600 focus:ring-blue-600 w-4 h-4"
                />
                <span>保存对方 Ed25519 公钥指纹，下次在同一局域网内自动配对重连</span>
              </label>

              <button
                type="submit"
                disabled={isPairing || (pairMode === 'discovered' && discoveredPeers.length === 0)}
                className="inline-flex items-center justify-center gap-1.5 px-4 py-2 text-xs font-medium rounded-lg bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-50 transition-colors whitespace-nowrap shrink-0"
              >
                <KeyRound className="w-3.5 h-3.5" />
                <span>{isPairing ? '正在加密握手...' : '连接并配对'}</span>
              </button>
            </div>
          </form>
        </section>
      </div>

      {/* Bottom Table: Trusted & Paired Devices with Auto-Pair Toggle */}
      <section
        aria-label="已配对信任设备与下次自动重连记录"
        className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl overflow-hidden"
      >
        <div className="p-5 border-b border-slate-200 dark:border-slate-800 flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3">
          <div className="flex items-center gap-2.5">
            <Wifi className="w-4 h-4 text-slate-500 shrink-0" />
            <div>
              <h2 className="text-base font-semibold text-slate-900 dark:text-slate-100">
                已记录信任主机列表（最多 {MAX_DEVICES} 台）
              </h2>
              <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5">
                开启「自动重连」后，已配对电脑在局域网广播上线时将自动通过 Ed25519 签名恢复加密会话。
              </p>
            </div>
          </div>

          <div className="relative w-full sm:w-64 shrink-0">
            <Search className="w-3.5 h-3.5 text-slate-400 absolute left-3 top-1/2 -translate-y-1/2 pointer-events-none" />
            <input
              type="search"
              aria-label="搜索已配对设备名称、IP 或操作系统"
              placeholder="搜索主机名称 / IP / 系统..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="w-full pl-8 pr-3 py-1.5 text-xs rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-slate-900 dark:text-slate-100 focus:outline-none focus:ring-2 focus:ring-blue-600"
            />
          </div>
        </div>

        <div className="overflow-x-auto">
          <table className="w-full text-left border-collapse">
            <thead>
              <tr className="border-b border-slate-200 dark:border-slate-800 text-xs text-slate-500 dark:text-slate-400">
                <th className="py-3 px-5 font-medium whitespace-nowrap">主机名称与角色</th>
                <th className="py-3 px-4 font-medium whitespace-nowrap">系统与显示规格</th>
                <th className="py-3 px-4 font-medium whitespace-nowrap">局域网地址与状态</th>
                <th className="py-3 px-4 font-medium whitespace-nowrap">互信公钥指纹</th>
                <th className="py-3 px-4 font-medium whitespace-nowrap">下次自动配对</th>
                <th className="py-3 px-5 font-medium text-right whitespace-nowrap">操作</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-200 dark:divide-slate-800 text-xs">
              {filteredTrustedNodes.length === 0 ? (
                <tr>
                  <td
                    colSpan={6}
                    className="py-8 px-5 text-center text-slate-500 dark:text-slate-400"
                  >
                    未找到匹配的信任设备，请调整搜索关键词。
                  </td>
                </tr>
              ) : (
                filteredTrustedNodes.map((node) => {
                  const isSelf = node.id === localNodeId;
                  return (
                    <tr
                      key={node.id}
                      className="hover:bg-slate-50/70 dark:hover:bg-slate-800/40 transition-colors"
                    >
                      <td className="py-3 px-5 whitespace-nowrap">
                        <div className="font-semibold text-slate-900 dark:text-slate-100">
                          {node.name}
                        </div>
                        <div className="text-slate-500 dark:text-slate-400 mt-0.5">
                          {isSelf
                            ? '本机节点'
                            : node.isPrimary
                              ? '主屏幕节点'
                              : `已配对从屏 · ${node.pairedAt}`}
                        </div>
                      </td>
                      <td className="py-3 px-4 text-slate-700 dark:text-slate-300 tabular-nums whitespace-nowrap">
                        {getOsLabel(node.os)} · {node.resolution.width}×{node.resolution.height} · {node.refreshRateHz}Hz
                      </td>
                      <td className="py-3 px-4 font-mono text-slate-700 dark:text-slate-300 tabular-nums whitespace-nowrap">
                        {node.ip} ·{' '}
                        <span
                          className={
                            node.online
                              ? 'text-emerald-700 dark:text-emerald-400'
                              : 'text-slate-400 dark:text-slate-500'
                          }
                        >
                          {node.online ? `${node.latencyMs.toFixed(1)} ms` : '离线'}
                        </span>
                      </td>
                      <td className="py-3 px-4 font-mono text-slate-500 dark:text-slate-400 tabular-nums whitespace-nowrap">
                        <span className="inline-flex items-center gap-1.5">
                          <ShieldCheck className="w-3.5 h-3.5 text-emerald-600 dark:text-emerald-400 shrink-0" />
                          {node.fingerprint}
                        </span>
                      </td>
                      <td className="py-3 px-4">
                        {isSelf ? (
                          <span className="text-slate-400 dark:text-slate-500">本机常驻</span>
                        ) : (
                          <button
                            type="button"
                            onClick={() => onToggleAutoPair(node.id)}
                            aria-label={`切换 ${node.name} 的下次自动配对状态`}
                            className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md border text-xs font-medium transition-colors whitespace-nowrap shrink-0 ${
                              node.autoPair
                                ? 'border-blue-200 dark:border-blue-800 bg-blue-50 dark:bg-blue-950/40 text-blue-700 dark:text-blue-300'
                                : 'border-slate-200 dark:border-slate-700 text-slate-500 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-800'
                            }`}
                          >
                            {node.autoPair && <Check className="w-3 h-3" />}
                            {node.autoPair ? '自动重连已开启' : '需手动确认'}
                          </button>
                        )}
                      </td>
                      <td className="py-3 px-5 text-right">
                        {isSelf || node.isPrimary ? (
                          <span className="text-slate-400 dark:text-slate-500">
                            {isSelf ? '本机不可移除' : '主屏不可移除'}
                          </span>
                        ) : (
                          <button
                            type="button"
                            onClick={() => onRemoveNode(node.id)}
                            aria-label={`解除并移除设备 ${node.name}`}
                            className="inline-flex items-center gap-1 px-2.5 py-1 rounded-md text-slate-500 hover:text-red-600 hover:bg-red-50 dark:hover:bg-red-950/30 transition-colors whitespace-nowrap shrink-0"
                          >
                            <Trash2 className="w-3.5 h-3.5" />
                            解除配对
                          </button>
                        )}
                      </td>
                    </tr>
                  );
                })
              )}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
};
