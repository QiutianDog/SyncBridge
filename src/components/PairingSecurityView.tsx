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
} from 'lucide-react';
import {
  DeviceNode,
  DiscoveredPeer,
  MAX_DEVICES,
  getOsLabel,
  validatePairingPin,
} from '../utils/kvmEngine';

interface PairingSecurityViewProps {
  nodes: DeviceNode[];
  discoveredPeers: DiscoveredPeer[];
  localEphemeralPin: string;
  onRefreshLocalPin: () => void;
  onPairPeer: (
    peer: DiscoveredPeer,
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
  onRefreshLocalPin,
  onPairPeer,
  onToggleAutoPair,
  onRemoveNode,
}) => {
  const [selectedPeerId, setSelectedPeerId] = useState<string>(
    discoveredPeers[0]?.id ?? ''
  );
  const [pinInput, setPinInput] = useState<string>('');
  const [rememberAutoPair, setRememberAutoPair] = useState<boolean>(true);
  const [formFeedback, setFormFeedback] = useState<{
    tone: 'success' | 'error';
    text: string;
  } | null>(null);
  const [searchQuery, setSearchQuery] = useState<string>('');

  const activePeer =
    discoveredPeers.find((p) => p.id === selectedPeerId) ?? discoveredPeers[0];

  const handlePairSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!activePeer) {
      setFormFeedback({ tone: 'error', text: '当前局域网暂无未配对的待选设备。' });
      return;
    }

    const pinCheck = validatePairingPin(pinInput);
    if (!pinCheck.valid) {
      setFormFeedback({
        tone: 'error',
        text: pinCheck.error ?? '请输入有效的 6 位数字临时配对密钥。',
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
        rememberAutoPair ? '，已记录指纹并开启下次自动配对' : ''
      }。`,
    });
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
          局域网发现、临时密钥配对与加密连接
        </h1>
        <p className="text-sm text-slate-600 dark:text-slate-400 mt-1">
          严格限定局域网（mDNS 子网绑定，无公网中转）。首次连接使用 6 位一次性临时密钥完成 SPAKE2 验证并签发 Ed25519 互信证书，后续上线自动加密重连。
        </p>
      </div>

      {/* Top Row: Local Ephemeral Key Generator + Remote Peer Pairing Form */}
      <div className="grid grid-cols-1 lg:grid-cols-12 gap-6 items-stretch">
        {/* Left 5 Cols: This Machine's Temporary Pairing Key */}
        <section
          aria-label="本机局域网临时配对密钥"
          className="lg:col-span-5 bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl p-6 flex flex-col justify-between"
        >
          <div>
            <div className="flex items-center justify-between gap-2">
              <span className="text-xs font-medium text-slate-500 dark:text-slate-400">
                本机被动配对凭证 · 5 分钟自动轮换
              </span>
              <Lock className="w-4 h-4 text-blue-600 dark:text-blue-400 shrink-0" />
            </div>

            <div className="mt-4 py-5 px-4 rounded-lg bg-slate-50 dark:bg-slate-950 border border-slate-200/80 dark:border-slate-800 text-center">
              <div className="text-xs text-slate-500 dark:text-slate-400 mb-1">
                其他局域网电脑连接本机时输入此临时密钥
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
                <dt>网络隔离策略</dt>
                <dd className="font-mono text-slate-900 dark:text-slate-200 tabular-nums">
                  仅限本地局域网 (192.168.50.0/24)
                </dd>
              </div>
              <div className="flex justify-between gap-4">
                <dt>传输层安全套件</dt>
                <dd className="font-mono text-slate-900 dark:text-slate-200">
                  QUIC + TLS 1.3 · ChaCha20-Poly1305
                </dd>
              </div>
              <div className="flex justify-between gap-4">
                <dt>互联席位使用量</dt>
                <dd className="font-mono text-slate-900 dark:text-slate-200 tabular-nums">
                  {nodes.length} / {MAX_DEVICES} 台电脑
                </dd>
              </div>
            </dl>
          </div>

          <div className="mt-5 pt-4 border-t border-slate-100 dark:border-slate-800 flex items-center justify-between gap-3">
            <span className="text-xs text-slate-500 dark:text-slate-400">
              配对一次后即固化公钥指纹，原临时码立即失效
            </span>
            <button
              type="button"
              onClick={onRefreshLocalPin}
              className="inline-flex items-center gap-1.5 px-3.5 py-2 text-xs font-medium rounded-lg border border-slate-200 dark:border-slate-700 text-slate-700 dark:text-slate-200 hover:bg-slate-50 dark:hover:bg-slate-800 transition-colors whitespace-nowrap shrink-0"
            >
              <RefreshCw className="w-3.5 h-3.5" />
              刷新临时密钥
            </button>
          </div>
        </section>

        {/* Right 7 Cols: Pair New Discovered LAN Computer */}
        <section
          aria-label="通过临时密钥配对局域网设备"
          className="lg:col-span-7 bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl p-6 flex flex-col justify-between"
        >
          <form onSubmit={handlePairSubmit} className="space-y-4">
            <div className="flex items-center justify-between gap-3">
              <div>
                <h2 className="text-base font-semibold text-slate-900 dark:text-slate-100">
                  主动配对局域网内发现的电脑（mDNS 广播发现）
                </h2>
                <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5">
                  选择同一局域网下的目标主机，输入对方界面展示的 6 位临时密钥完成首次加密握手。
                </p>
              </div>
              <Wifi className="w-4 h-4 text-slate-500 dark:text-slate-400 shrink-0" />
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

            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <div>
                <label
                  htmlFor="discovered-peer-select"
                  className="block text-xs font-medium text-slate-700 dark:text-slate-300 mb-1.5"
                >
                  局域网待配对电脑 ({discoveredPeers.length} 台在线可发现)
                </label>
                <select
                  id="discovered-peer-select"
                  value={activePeer?.id ?? ''}
                  onChange={(e) => setSelectedPeerId(e.target.value)}
                  disabled={discoveredPeers.length === 0}
                  className="w-full rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-950 pl-3 pr-8 py-2 text-xs font-medium text-slate-900 dark:text-slate-100 focus:outline-none focus:ring-2 focus:ring-blue-600"
                >
                  {discoveredPeers.length === 0 ? (
                    <option value="">局域网内所有已发现设备均已配对</option>
                  ) : (
                    discoveredPeers.map((peer) => (
                      <option key={peer.id} value={peer.id}>
                        {peer.name} ({peer.ip})
                      </option>
                    ))
                  )}
                </select>
              </div>

              <div>
                <div className="flex items-center justify-between mb-1.5">
                  <label
                    htmlFor="pairing-pin-input"
                    className="block text-xs font-medium text-slate-700 dark:text-slate-300"
                  >
                    对方 6 位临时配对密钥
                  </label>
                  <button
                    type="button"
                    onClick={() => setPinInput('628-409')}
                    className="text-xs text-blue-600 dark:text-blue-400 hover:underline whitespace-nowrap shrink-0"
                  >
                    填入示例码 628-409
                  </button>
                </div>
                <input
                  id="pairing-pin-input"
                  type="text"
                  inputMode="numeric"
                  placeholder="例如：628-409"
                  value={pinInput}
                  onChange={(e) => setPinInput(e.target.value)}
                  className="w-full rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-950 px-3 py-2 text-xs font-mono text-slate-900 dark:text-slate-100 tabular-nums focus:outline-none focus:ring-2 focus:ring-blue-600"
                />
              </div>
            </div>

            <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 pt-2">
              <label className="inline-flex items-center gap-2.5 text-xs text-slate-700 dark:text-slate-300 cursor-pointer">
                <input
                  type="checkbox"
                  checked={rememberAutoPair}
                  onChange={(e) => setRememberAutoPair(e.target.checked)}
                  className="rounded border-slate-300 text-blue-600 focus:ring-blue-600 w-4 h-4"
                />
                <span>记录对方公钥指纹至系统密钥链，下次同一局域网自动配对重连</span>
              </label>

              <button
                type="submit"
                disabled={discoveredPeers.length === 0}
                className="inline-flex items-center justify-center gap-1.5 px-4 py-2 text-xs font-medium rounded-lg bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-50 transition-colors whitespace-nowrap shrink-0"
              >
                <KeyRound className="w-3.5 h-3.5" />
                验证临时密钥并配对
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
          <div>
            <h2 className="text-base font-semibold text-slate-900 dark:text-slate-100">
              已记录信任主机列表（支持自动配对重连 · 最多 {MAX_DEVICES} 台）
            </h2>
            <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5">
              管理已完成首次临时密钥验证的主机指纹；开启自动配对后，局域网唤醒将在 1-RTT 内静默恢复加密会话。
            </p>
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
                <th className="py-3 px-4 font-medium whitespace-nowrap">系统与高刷规格</th>
                <th className="py-3 px-4 font-medium whitespace-nowrap">局域网地址与延迟</th>
                <th className="py-3 px-4 font-medium whitespace-nowrap">互信证书指纹 (TLS 1.3)</th>
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
                filteredTrustedNodes.map((node) => (
                  <tr
                    key={node.id}
                    className="hover:bg-slate-50/70 dark:hover:bg-slate-800/40 transition-colors"
                  >
                    <td className="py-3 px-5 whitespace-nowrap">
                      <div className="font-semibold text-slate-900 dark:text-slate-100">
                        {node.name}
                      </div>
                      <div className="text-slate-500 dark:text-slate-400 mt-0.5">
                        {node.isPrimary ? '主屏幕 · 本机键鼠宿主' : `受控从屏 · 配对时间 ${node.pairedAt}`}
                      </div>
                    </td>
                    <td className="py-3 px-4 text-slate-700 dark:text-slate-300 tabular-nums whitespace-nowrap">
                      {getOsLabel(node.os)} · {node.resolution.width}×{node.resolution.height} · {node.refreshRateHz}Hz
                    </td>
                    <td className="py-3 px-4 font-mono text-slate-700 dark:text-slate-300 tabular-nums whitespace-nowrap">
                      {node.ip} · {node.latencyMs.toFixed(1)} ms
                    </td>
                    <td className="py-3 px-4 font-mono text-slate-500 dark:text-slate-400 tabular-nums whitespace-nowrap">
                      <span className="inline-flex items-center gap-1.5">
                        <ShieldCheck className="w-3.5 h-3.5 text-emerald-600 dark:text-emerald-400 shrink-0" />
                        {node.fingerprint}
                      </span>
                    </td>
                    <td className="py-3 px-4">
                      {node.isPrimary ? (
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
                          {node.autoPair ? '已开启自动重连' : '需每次确认'}
                        </button>
                      )}
                    </td>
                    <td className="py-3 px-5 text-right">
                      {node.isPrimary ? (
                        <span className="text-slate-400 dark:text-slate-500">不可移除主屏</span>
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
                ))
              )}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
};
