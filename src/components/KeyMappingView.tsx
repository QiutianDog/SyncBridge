import React, { useState } from 'react';
import { Keyboard, Plus, Trash2, ArrowRight } from 'lucide-react';
import {
  DeviceNode,
  KeyMappingRule,
  OsFamily,
  getOsLabel,
  resolveMappedKey,
  validateKeyMappingRule,
} from '../utils/kvmEngine';

interface KeyMappingViewProps {
  rules: KeyMappingRule[];
  nodes: DeviceNode[];
  activeFocusNodeId: string;
  onToggleRule: (ruleId: string) => void;
  onDeleteRule: (ruleId: string) => void;
  onAddRule: (rule: Omit<KeyMappingRule, 'id'>) => void;
}

const KEY_OPTIONS = [
  'Command (⌘)',
  'Option (⌥)',
  'Control (⌃)',
  'Ctrl',
  'Alt',
  'Win / Super',
  'CapsLock',
  'Escape',
  'Backspace',
  'Delete',
  'PrintScreen',
  'F13',
  'F14',
  'Tab',
];

export const KeyMappingView: React.FC<KeyMappingViewProps> = ({
  rules,
  nodes,
  activeFocusNodeId,
  onToggleRule,
  onDeleteRule,
  onAddRule,
}) => {
  const [osFilter, setOsFilter] = useState<'all' | OsFamily>('all');
  const [newLabel, setNewLabel] = useState<string>('');
  const [newTargetOs, setNewTargetOs] = useState<'all' | OsFamily>('windows');
  const [newSourceKey, setNewSourceKey] = useState<string>('Control (⌃)');
  const [newTargetKey, setNewTargetKey] = useState<string>('Win / Super');
  const [ruleError, setRuleError] = useState<string | null>(null);

  const [testerDeviceId, setTesterDeviceId] = useState<string>(activeFocusNodeId);
  const [testerPhysicalKey, setTesterPhysicalKey] = useState<string>('Command (⌘)');

  const filteredRules = rules.filter((r) =>
    osFilter === 'all' ? true : r.targetOs === osFilter || r.targetOs === 'all'
  );

  const handleCreateRule = (e: React.FormEvent) => {
    e.preventDefault();
    const labelText =
      newLabel.trim() ||
      `${newTargetOs === 'all' ? '全局' : newTargetOs}：${newSourceKey} 映射为 ${newTargetKey}`;

    const candidate: Omit<KeyMappingRule, 'id'> = {
      label: labelText,
      targetOs: newTargetOs,
      sourceKey: newSourceKey,
      targetKey: newTargetKey,
      enabled: true,
    };

    const check = validateKeyMappingRule(rules, candidate);
    if (!check.ok) {
      setRuleError(check.error ?? '规则校验未通过。');
      return;
    }

    setRuleError(null);
    onAddRule(candidate);
    setNewLabel('');
  };

  const targetTestNode =
    nodes.find((n) => n.id === testerDeviceId) ?? nodes[0];
  const resolvedTest = targetTestNode
    ? resolveMappedKey(rules, testerPhysicalKey, targetTestNode.os)
    : { outputKey: testerPhysicalKey };

  const formatTargetOsName = (os: 'all' | OsFamily) => {
    if (os === 'all') return '全部互联系统';
    if (os === 'macos') return 'macOS 节点';
    if (os === 'windows') return 'Windows 节点';
    return 'Linux (Wayland / X11) 节点';
  };

  return (
    <div className="space-y-6">
      <div className="pb-5 border-b border-slate-200 dark:border-slate-800">
        <h1 className="text-xl font-semibold tracking-tight text-slate-900 dark:text-slate-100">
          跨系统键盘部分按键映射
        </h1>
        <p className="text-sm text-slate-600 dark:text-slate-400 mt-1">
          解决 Mac、Windows 与 Linux 混用一套物理键盘时的修饰键差异（如 Command ⌘ 与 Ctrl 互换、CapsLock 映射 Escape），仅对鼠标当前所在屏幕动态生效。
        </p>
      </div>

      <div className="grid grid-cols-1 xl:grid-cols-12 gap-6 items-start">
        {/* Left 7 Cols: Active Key Mapping Rules Table */}
        <section
          aria-label="按键映射规则列表"
          className="xl:col-span-7 bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl overflow-hidden"
        >
          <div className="p-5 border-b border-slate-200 dark:border-slate-800 flex flex-wrap items-center justify-between gap-3">
            <h2 className="text-base font-semibold text-slate-900 dark:text-slate-100">
              按键重映射规则表 ({filteredRules.length} 条)
            </h2>

            {/* Interactive Segmented Filter */}
            <div
              role="group"
              aria-label="按目标操作系统筛选映射规则"
              className="flex items-center gap-1 p-1 rounded-lg bg-slate-100 dark:bg-slate-800"
            >
              {(
                [
                  { id: 'all', label: '全部' },
                  { id: 'windows', label: 'Windows' },
                  { id: 'linux', label: 'Linux' },
                  { id: 'macos', label: 'macOS' },
                ] as const
              ).map((tab) => (
                <button
                  key={tab.id}
                  type="button"
                  onClick={() => setOsFilter(tab.id)}
                  className={`px-2.5 py-1 text-xs font-medium rounded-md transition-colors whitespace-nowrap shrink-0 ${
                    osFilter === tab.id
                      ? 'bg-white dark:bg-slate-900 text-slate-900 dark:text-slate-100 shadow-xs'
                      : 'text-slate-600 dark:text-slate-400 hover:text-slate-900 dark:hover:text-slate-200'
                  }`}
                >
                  {tab.label}
                </button>
              ))}
            </div>
          </div>

          <div className="divide-y divide-slate-200 dark:divide-slate-800">
            {filteredRules.length === 0 ? (
              <div className="p-8 text-center text-sm text-slate-500 dark:text-slate-400">
                当前系统筛选条件下暂无按键映射规则，可在右侧创建新规则。
              </div>
            ) : (
              filteredRules.map((rule) => (
                <div
                  key={rule.id}
                  className="px-5 py-3.5 flex items-center justify-between gap-4 hover:bg-slate-50/60 dark:hover:bg-slate-800/40 transition-colors"
                >
                  <div className="min-w-0">
                    <div className="text-sm font-medium text-slate-900 dark:text-slate-100">
                      {rule.label}
                    </div>
                    <div className="text-xs text-slate-500 dark:text-slate-400 mt-1 flex flex-wrap items-center gap-2">
                      <span>生效目标：{formatTargetOsName(rule.targetOs)}</span>
                      <span aria-hidden="true">·</span>
                      <span className="font-mono text-slate-700 dark:text-slate-300">
                        {rule.sourceKey} → {rule.targetKey}
                      </span>
                    </div>
                  </div>

                  <div className="flex items-center gap-2 shrink-0">
                    <button
                      type="button"
                      onClick={() => onToggleRule(rule.id)}
                      aria-label={`${rule.enabled ? '停用' : '启用'}规则 ${rule.label}`}
                      className={`px-3 py-1 text-xs font-medium rounded-md border transition-colors whitespace-nowrap shrink-0 ${
                        rule.enabled
                          ? 'border-blue-200 dark:border-blue-800 bg-blue-50 dark:bg-blue-950/40 text-blue-700 dark:text-blue-300'
                          : 'border-slate-200 dark:border-slate-700 text-slate-500 dark:text-slate-400'
                      }`}
                    >
                      {rule.enabled ? '已启用' : '已停用'}
                    </button>
                    <button
                      type="button"
                      onClick={() => onDeleteRule(rule.id)}
                      aria-label={`删除规则 ${rule.label}`}
                      className="p-1.5 rounded-md text-slate-400 hover:text-red-600 hover:bg-red-50 dark:hover:bg-red-950/30 transition-colors shrink-0"
                    >
                      <Trash2 className="w-4 h-4" />
                    </button>
                  </div>
                </div>
              ))
            )}
          </div>
        </section>

        {/* Right 5 Cols: Add Custom Key Rule + Live Key Translator */}
        <div className="xl:col-span-5 space-y-6">
          <section
            aria-label="新增自定义按键映射"
            className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl p-5"
          >
            <h2 className="text-base font-semibold text-slate-900 dark:text-slate-100">
              新增按键映射规则
            </h2>
            <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5">
              在 Rust 核心层拦截主屏幕 HID 扫描码，按鼠标焦点所在系统零延迟转换后发送。
            </p>

            <form onSubmit={handleCreateRule} className="mt-4 space-y-3.5">
              {ruleError && (
                <div
                  role="alert"
                  className="px-3 py-2 rounded-lg border border-red-200 dark:border-red-900 bg-red-50 dark:bg-red-950/30 text-xs text-red-700 dark:text-red-300"
                >
                  {ruleError}
                </div>
              )}

              <div>
                <label
                  htmlFor="rule-target-os"
                  className="block text-xs font-medium text-slate-700 dark:text-slate-300 mb-1"
                >
                  生效目标操作系统
                </label>
                <select
                  id="rule-target-os"
                  value={newTargetOs}
                  onChange={(e) => setNewTargetOs(e.target.value as 'all' | OsFamily)}
                  className="w-full rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-950 px-3 py-2 text-xs text-slate-900 dark:text-slate-100"
                >
                  <option value="windows">仅当鼠标在 Windows 屏幕时生效</option>
                  <option value="linux">仅当鼠标在 Linux 屏幕时生效</option>
                  <option value="macos">仅当鼠标在 macOS 屏幕时生效</option>
                  <option value="all">全局所有屏幕生效</option>
                </select>
              </div>

              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label
                    htmlFor="rule-source-key"
                    className="block text-xs font-medium text-slate-700 dark:text-slate-300 mb-1"
                  >
                    物理按下按键（源键）
                  </label>
                  <select
                    id="rule-source-key"
                    value={newSourceKey}
                    onChange={(e) => setNewSourceKey(e.target.value)}
                    className="w-full rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-950 px-3 py-2 text-xs font-mono text-slate-900 dark:text-slate-100"
                  >
                    {KEY_OPTIONS.map((k) => (
                      <option key={k} value={k}>
                        {k}
                      </option>
                    ))}
                  </select>
                </div>

                <div>
                  <label
                    htmlFor="rule-target-key"
                    className="block text-xs font-medium text-slate-700 dark:text-slate-300 mb-1"
                  >
                    目标系统注入按键
                  </label>
                  <select
                    id="rule-target-key"
                    value={newTargetKey}
                    onChange={(e) => setNewTargetKey(e.target.value)}
                    className="w-full rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-950 px-3 py-2 text-xs font-mono text-slate-900 dark:text-slate-100"
                  >
                    {KEY_OPTIONS.map((k) => (
                      <option key={k} value={k}>
                        {k}
                      </option>
                    ))}
                  </select>
                </div>
              </div>

              <div>
                <label
                  htmlFor="rule-custom-label"
                  className="block text-xs font-medium text-slate-700 dark:text-slate-300 mb-1"
                >
                  规则备注名称（选填）
                </label>
                <input
                  id="rule-custom-label"
                  type="text"
                  placeholder="例如：Control 映射为 Windows 徽标键"
                  value={newLabel}
                  onChange={(e) => setNewLabel(e.target.value)}
                  className="w-full rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-950 px-3 py-2 text-xs text-slate-900 dark:text-slate-100"
                />
              </div>

              <button
                type="submit"
                className="w-full inline-flex items-center justify-center gap-1.5 px-4 py-2 text-xs font-medium rounded-lg bg-blue-600 text-white hover:bg-blue-700 transition-colors whitespace-nowrap shrink-0"
              >
                <Plus className="w-3.5 h-3.5" />
                添加按键映射规则
              </button>
            </form>
          </section>

          {/* Interactive Key Mapping Validator */}
          <section
            aria-label="按键映射输出校验"
            className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl p-5 space-y-3"
          >
            <div className="flex items-center gap-2">
              <Keyboard className="w-4 h-4 text-blue-600 dark:text-blue-400 shrink-0" />
              <h3 className="text-sm font-semibold text-slate-900 dark:text-slate-100">
                目标系统按键输出即时核对
              </h3>
            </div>

            <div className="grid grid-cols-2 gap-3">
              <div>
                <label
                  htmlFor="sandbox-device"
                  className="block text-xs text-slate-500 dark:text-slate-400 mb-1"
                >
                  假设鼠标当前位于屏幕
                </label>
                <select
                  id="sandbox-device"
                  value={targetTestNode?.id ?? ''}
                  onChange={(e) => setTesterDeviceId(e.target.value)}
                  className="w-full rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 px-2.5 py-1.5 text-xs text-slate-900 dark:text-slate-100"
                >
                  {nodes.map((n) => (
                    <option key={n.id} value={n.id}>
                      {n.name} ({getOsLabel(n.os)})
                    </option>
                  ))}
                </select>
              </div>

              <div>
                <label
                  htmlFor="sandbox-key"
                  className="block text-xs text-slate-500 dark:text-slate-400 mb-1"
                >
                  主键盘按下物理键
                </label>
                <select
                  id="sandbox-key"
                  value={testerPhysicalKey}
                  onChange={(e) => setTesterPhysicalKey(e.target.value)}
                  className="w-full rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 px-2.5 py-1.5 text-xs font-mono text-slate-900 dark:text-slate-100"
                >
                  {KEY_OPTIONS.map((k) => (
                    <option key={k} value={k}>
                      {k}
                    </option>
                  ))}
                </select>
              </div>
            </div>

            <div className="pt-2 border-t border-slate-100 dark:border-slate-800 flex items-center justify-between text-xs font-mono">
              <span className="text-slate-600 dark:text-slate-400">{testerPhysicalKey}</span>
              <ArrowRight className="w-3.5 h-3.5 text-slate-400 shrink-0" />
              <span className="font-semibold text-blue-700 dark:text-blue-400">
                注入 {targetTestNode?.name}：{resolvedTest.outputKey}
              </span>
            </div>
            <div className="text-[11px] text-slate-500 dark:text-slate-400">
              {resolvedTest.matchedRule
                ? `命中规则：${resolvedTest.matchedRule.label}`
                : '未命中重映射规则：保持原始 HID 键位透传'}
            </div>
          </section>
        </div>
      </div>
    </div>
  );
};
