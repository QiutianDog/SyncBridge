import React, { useEffect, useState } from 'react';
import {
  AppWindow,
  Check,
  Laptop,
  MousePointerClick,
  Network,
  ShieldCheck,
} from 'lucide-react';
import type {
  EngineTuning,
  LinuxWaylandStrategy,
  LocalSystemStatus,
} from '../utils/kvmEngine';
import { getOsLabel } from '../utils/kvmEngine';

interface SystemSettingsViewProps {
  tuning: EngineTuning;
  systemStatus: LocalSystemStatus;
  activeDriverName: string;
  onUpdateTuning: (patch: Partial<EngineTuning>) => Promise<void>;
  onRequestInputPermission: () => Promise<void>;
}

export const SystemSettingsView: React.FC<SystemSettingsViewProps> = ({
  tuning,
  systemStatus,
  activeDriverName,
  onUpdateTuning,
  onRequestInputPermission,
}) => {
  const [deviceNameDraft, setDeviceNameDraft] = useState<string>(
    tuning.localDeviceName
  );
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    setDeviceNameDraft(tuning.localDeviceName);
  }, [tuning.localDeviceName]);

  const isMacOs = systemStatus.localOs === 'macos';
  const isLinux =
    systemStatus.localOs === 'linux-wayland' ||
    systemStatus.localOs === 'linux-x11';

  const handleSaveDeviceName = async (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = deviceNameDraft.trim();
    if (!trimmed) {
      setNotice('设备显示名称不能为空。');
      return;
    }
    await onUpdateTuning({ localDeviceName: trimmed });
    setNotice(`本机局域网广播名称已更新为「${trimmed}」。`);
  };

  const handleToggleHideDock = async () => {
    const nextHide = !tuning.hideDockIcon;
    await onUpdateTuning({ hideDockIcon: nextHide });
    setNotice(
      nextHide
        ? '已开启隐藏程序坞 (Dock) 图标：关闭主窗口后仅在顶部状态栏托盘驻留，左键点击托盘图标可随时唤出。'
        : '已恢复显示程序坞 (Dock) 图标（默认状态）。'
    );
  };

  const handleWaylandStrategyChange = async (strategy: LinuxWaylandStrategy) => {
    await onUpdateTuning({ linuxWaylandStrategy: strategy });
    setNotice('已保存 Linux Wayland 输入驱动策略。');
  };

  return (
    <div className="space-y-6">
      <div className="pb-5 border-b border-slate-200 dark:border-slate-800">
        <h1 className="text-xl font-semibold tracking-tight text-slate-900 dark:text-slate-100">
          系统权限、托盘行为与本机设置
        </h1>
        <p className="text-sm text-slate-600 dark:text-slate-400 mt-1">
          管理系统托盘交互方式、macOS 程序坞 (Dock) 图标显示策略、跨平台输入捕获权限以及局域网身份参数。
        </p>
      </div>

      {notice && (
        <div
          role="status"
          className="px-4 py-2.5 rounded-lg border border-blue-200 dark:border-blue-900 bg-blue-50/70 dark:bg-blue-950/30 text-xs text-slate-800 dark:text-slate-200 flex items-center justify-between gap-4"
        >
          <span>{notice}</span>
          <button
            type="button"
            onClick={() => setNotice(null)}
            className="text-slate-500 hover:text-slate-800 dark:text-slate-400 dark:hover:text-slate-200 whitespace-nowrap shrink-0"
          >
            关闭提示
          </button>
        </div>
      )}

      <div className="grid grid-cols-1 lg:grid-cols-12 gap-6 items-start">
        {/* Left 7 Cols: System Tray Behavior & macOS Dock Visibility */}
        <div className="lg:col-span-7 space-y-6">
          <section
            aria-label="系统托盘与程序坞驻留设置"
            className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl divide-y divide-slate-200 dark:divide-slate-800"
          >
            <div className="p-5">
              <div className="flex items-center justify-between gap-3">
                <div>
                  <h2 className="text-base font-semibold text-slate-900 dark:text-slate-100">
                    系统托盘交互与后台驻留
                  </h2>
                  <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5">
                    点击窗口关闭按钮时，软件不会中断跨屏键鼠连接，而是自动转入系统托盘后台运行。
                  </p>
                </div>
                <MousePointerClick className="w-4 h-4 text-blue-600 dark:text-blue-400 shrink-0" />
              </div>

              <dl className="mt-4 space-y-2.5 text-xs border-t border-slate-100 dark:border-slate-800 pt-4">
                <div className="flex items-center justify-between gap-4">
                  <dt className="text-slate-600 dark:text-slate-400">
                    左键点击系统托盘图标
                  </dt>
                  <dd className="font-medium text-slate-900 dark:text-slate-100">
                    直接展示并聚焦 SpanKVM 主窗口
                  </dd>
                </div>
                <div className="flex items-center justify-between gap-4">
                  <dt className="text-slate-600 dark:text-slate-400">
                    右键点击系统托盘图标
                  </dt>
                  <dd className="font-medium text-slate-900 dark:text-slate-100">
                    弹出快捷菜单（暂停共享 / 回收焦点 / 退出）
                  </dd>
                </div>
                <div className="flex items-center justify-between gap-4">
                  <dt className="text-slate-600 dark:text-slate-400">
                    点击窗口关闭按钮
                  </dt>
                  <dd className="font-medium text-slate-900 dark:text-slate-100">
                    最小化隐藏至系统托盘（保持跨屏连接不断开）
                  </dd>
                </div>
              </dl>
            </div>

            {/* macOS Hide Dock Icon Toggle (default: false) */}
            <div className="p-5 flex items-start justify-between gap-4">
              <div className="space-y-1">
                <div className="flex items-center gap-2">
                  <AppWindow className="w-4 h-4 text-slate-600 dark:text-slate-300 shrink-0" />
                  <span className="text-sm font-semibold text-slate-900 dark:text-slate-100">
                    macOS 隐藏程序坞 (Dock) 图标
                  </span>
                </div>
                <p className="text-xs text-slate-500 dark:text-slate-400">
                  默认不隐藏。开启后，macOS 版将隐藏底部程序坞 (Dock) 图标，仅保留右上角状态栏托盘图标（左键点击托盘可随时打开界面）。
                  {!isMacOs && '（此配置在 macOS 系统运行时生效）'}
                </p>
              </div>

              <button
                type="button"
                role="switch"
                aria-checked={tuning.hideDockIcon}
                aria-label="隐藏程序坞 (Dock) 图标"
                onClick={handleToggleHideDock}
                className={`inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg border text-xs font-medium transition-colors whitespace-nowrap shrink-0 ${
                  tuning.hideDockIcon
                    ? 'border-blue-600 bg-blue-600 text-white hover:bg-blue-700'
                    : 'border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-slate-700 dark:text-slate-200 hover:bg-slate-100 dark:hover:bg-slate-800'
                }`}
              >
                {tuning.hideDockIcon && <Check className="w-3.5 h-3.5" />}
                <span>
                  {tuning.hideDockIcon ? '已隐藏 Dock 图标' : '未隐藏（默认）'}
                </span>
              </button>
            </div>
          </section>

          {/* OS Input Capture Permission & Wayland Strategy */}
          <section
            aria-label="系统底层输入驱动与权限状态"
            className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl divide-y divide-slate-200 dark:divide-slate-800"
          >
            <div className="p-5 space-y-3">
              <div className="flex items-center justify-between gap-3">
                <div>
                  <h2 className="text-base font-semibold text-slate-900 dark:text-slate-100">
                    跨平台底层输入捕获与注入权限
                  </h2>
                  <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5">
                    当前加载驱动：{activeDriverName}
                  </p>
                </div>
                <ShieldCheck className="w-4 h-4 text-blue-600 dark:text-blue-400 shrink-0" />
              </div>

              <div className="pt-2 flex flex-col sm:flex-row sm:items-center justify-between gap-3">
                <div className="text-xs">
                  <span className="text-slate-500 dark:text-slate-400">
                    系统输入控制权限状态：
                  </span>
                  <span
                    className={
                      systemStatus.inputPermissionGranted
                        ? 'font-semibold text-emerald-700 dark:text-emerald-400'
                        : 'font-semibold text-amber-700 dark:text-amber-400'
                    }
                  >
                    {systemStatus.inputPermissionGranted
                      ? '已就绪（支持全局键鼠捕获与远端注入）'
                      : '尚未授权（macOS 需在隐私与安全性 -> 辅助功能中勾选 SpanKVM）'}
                  </span>
                </div>

                <button
                  type="button"
                  onClick={onRequestInputPermission}
                  className="px-3.5 py-2 text-xs font-medium rounded-lg border border-slate-200 dark:border-slate-700 text-slate-700 dark:text-slate-200 hover:bg-slate-50 dark:hover:bg-slate-800 transition-colors whitespace-nowrap shrink-0"
                >
                  打开系统辅助功能权限设置
                </button>
              </div>
            </div>

            <div className="p-5 space-y-3">
              <div>
                <h3 className="text-sm font-semibold text-slate-900 dark:text-slate-100">
                  Linux Wayland 输入后端策略
                </h3>
                <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5">
                  针对 GNOME 46+、KDE Plasma 6、Hyprland 与 Sway 等 Wayland 桌面环境的兼容模式选择。
                  {!isLinux && '（当连接或运行于 Linux 主机时生效）'}
                </p>
              </div>

              <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                {(
                  [
                    {
                      id: 'portal-and-uinput-fallback',
                      title: 'Portal (libei) + /dev/uinput 双栈',
                      desc: '优先走 XDG InputCapture Portal，合成器不支持时自动回退内核 uinput 虚拟键鼠（推荐）',
                    },
                    {
                      id: 'portal-only',
                      title: '仅使用 Wayland Portal (libei)',
                      desc: '严格通过桌面合成器原生授权会话捕获与注入，不使用 /dev/uinput 设备节点',
                    },
                  ] as const
                ).map((opt) => {
                  const selected = tuning.linuxWaylandStrategy === opt.id;
                  return (
                    <button
                      key={opt.id}
                      type="button"
                      onClick={() => handleWaylandStrategyChange(opt.id)}
                      className={`text-left rounded-lg border p-3.5 transition-colors ${
                        selected
                          ? 'border-blue-600 dark:border-blue-500 bg-blue-50/50 dark:bg-blue-950/30'
                          : 'border-slate-200 dark:border-slate-800 hover:border-slate-300 dark:hover:border-slate-700'
                      }`}
                    >
                      <div className="text-xs font-semibold text-slate-900 dark:text-slate-100">
                        {opt.title}
                      </div>
                      <div className="text-xs text-slate-500 dark:text-slate-400 mt-1">
                        {opt.desc}
                      </div>
                    </button>
                  );
                })}
              </div>
            </div>
          </section>
        </div>

        {/* Right 5 Cols: Local Device Identity & Network Endpoint Info */}
        <div className="lg:col-span-5 space-y-6">
          <section
            aria-label="本机设备名称与局域网通信参数"
            className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl divide-y divide-slate-200 dark:divide-slate-800"
          >
            <form onSubmit={handleSaveDeviceName} className="p-5 space-y-3">
              <div className="flex items-center justify-between gap-2">
                <h2 className="text-base font-semibold text-slate-900 dark:text-slate-100">
                  本机局域网标识
                </h2>
                <Laptop className="w-4 h-4 text-slate-500 dark:text-slate-400 shrink-0" />
              </div>

              <div>
                <label
                  htmlFor="local-device-name-input"
                  className="block text-xs font-medium text-slate-700 dark:text-slate-300 mb-1.5"
                >
                  在其他电脑上显示的本机名称
                </label>
                <div className="flex gap-2">
                  <input
                    id="local-device-name-input"
                    type="text"
                    value={deviceNameDraft}
                    onChange={(e) => setDeviceNameDraft(e.target.value)}
                    placeholder="例如：MacBook-Pro-Studio"
                    className="flex-1 rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-950 px-3 py-2 text-xs text-slate-900 dark:text-slate-100 focus:outline-none focus:ring-2 focus:ring-blue-600"
                  />
                  <button
                    type="submit"
                    className="px-4 py-2 text-xs font-medium rounded-lg bg-blue-600 text-white hover:bg-blue-700 transition-colors whitespace-nowrap shrink-0"
                  >
                    保存名称
                  </button>
                </div>
              </div>
            </form>

            <div className="p-5 space-y-3">
              <div className="flex items-center justify-between gap-2">
                <h3 className="text-sm font-semibold text-slate-900 dark:text-slate-100">
                  网络监听端口与安全凭证
                </h3>
                <Network className="w-4 h-4 text-slate-500 dark:text-slate-400 shrink-0" />
              </div>

              <dl className="space-y-2.5 text-xs">
                <div className="flex items-center justify-between gap-4">
                  <dt className="text-slate-500 dark:text-slate-400">本机系统平台</dt>
                  <dd className="font-medium text-slate-900 dark:text-slate-100">
                    {getOsLabel(systemStatus.localOs)}
                  </dd>
                </div>
                <div className="flex items-center justify-between gap-4">
                  <dt className="text-slate-500 dark:text-slate-400">本机局域网 IPv4</dt>
                  <dd className="font-mono text-slate-900 dark:text-slate-100 tabular-nums">
                    {systemStatus.localIp}
                  </dd>
                </div>
                <div className="flex items-center justify-between gap-4">
                  <dt className="text-slate-500 dark:text-slate-400">
                    QUIC 加密键鼠与文件端口
                  </dt>
                  <dd className="font-mono text-slate-900 dark:text-slate-100 tabular-nums">
                    UDP {systemStatus.quicPort}
                  </dd>
                </div>
                <div className="flex items-center justify-between gap-4">
                  <dt className="text-slate-500 dark:text-slate-400">
                    局域网自动发现广播端口
                  </dt>
                  <dd className="font-mono text-slate-900 dark:text-slate-100 tabular-nums">
                    UDP {systemStatus.discoveryPort}
                  </dd>
                </div>
                <div className="flex items-center justify-between gap-4">
                  <dt className="text-slate-500 dark:text-slate-400">
                    本机 Ed25519 公钥指纹
                  </dt>
                  <dd className="font-mono text-slate-900 dark:text-slate-100 tabular-nums">
                    {systemStatus.fingerprint}
                  </dd>
                </div>
                <div className="flex items-center justify-between gap-4 pt-2 border-t border-slate-100 dark:border-slate-800">
                  <dt className="text-slate-500 dark:text-slate-400">
                    紧急切回本机焦点热键
                  </dt>
                  <dd className="font-mono font-medium text-blue-700 dark:text-blue-400">
                    {tuning.releaseHotkey}
                  </dd>
                </div>
              </dl>
            </div>
          </section>
        </div>
      </div>
    </div>
  );
};
