/**
 * @license
 * SPDX-License-Identifier: Apache-2.0
 *
 * Clean default local-host state and cross-OS key remapping rules.
 * In Tauri v2 native desktop builds, `src-tauri/src/commands.rs` probes the real local machine
 * (hostname, OS, LAN IPv4, display resolution/Hz, and Ed25519 identity) directly.
 */

import type {
  DeviceNode,
  DiscoveredPeer,
  KeyMappingRule,
  TransferRecord,
} from '../utils/kvmEngine.ts';

export const INITIAL_NODES: DeviceNode[] = [
  {
    id: 'node-local-host',
    name: 'MacBook-Pro',
    hostname: 'macbook-pro.local',
    os: 'macos',
    ip: '192.168.50.10',
    resolution: { width: 3024, height: 1964 },
    refreshRateHz: 120,
    isPrimary: true,
    gridPos: { col: 2, row: 2 },
    autoPair: true,
    online: true,
    latencyMs: 0.1,
    fingerprint: 'ED25519:9A:4C:E2:11',
    pairedAt: '本机设备',
  },
  {
    id: 'node-win-workstation',
    name: 'Win11-Workstation',
    hostname: 'win11-workstation.local',
    os: 'windows',
    ip: '192.168.50.18',
    resolution: { width: 3840, height: 2160 },
    refreshRateHz: 144,
    isPrimary: false,
    gridPos: { col: 3, row: 2 },
    autoPair: true,
    online: true,
    latencyMs: 0.5,
    fingerprint: 'ED25519:3D:88:7B:04',
    pairedAt: '今日 09:30',
  },
];

export const INITIAL_DISCOVERED_PEERS: DiscoveredPeer[] = [
  {
    id: 'node-ubuntu-wayland',
    name: 'Ubuntu-Wayland-Dev',
    hostname: 'ubuntu-2404.local',
    os: 'linux-wayland',
    ip: '192.168.50.26',
    quicPort: 24800,
    resolution: { width: 2560, height: 1440 },
    refreshRateHz: 144,
    latencyMs: 0.6,
    fingerprint: 'ED25519:71:F0:2C:69',
  },
  {
    id: 'node-mac-mini',
    name: 'Mac-mini-Server',
    hostname: 'mac-mini.local',
    os: 'macos',
    ip: '192.168.50.42',
    quicPort: 24800,
    resolution: { width: 2560, height: 1440 },
    refreshRateHz: 120,
    latencyMs: 0.5,
    fingerprint: 'ED25519:44:B2:10:9F',
  },
];

export const INITIAL_KEY_RULES: KeyMappingRule[] = [
  {
    id: 'rule-mac-cmd-to-win-ctrl',
    label: 'Mac ⌘ Command → Windows Ctrl（复制 / 粘贴 / 全选）',
    targetOs: 'windows',
    sourceKey: 'Command (⌘)',
    targetKey: 'Ctrl',
    enabled: true,
  },
  {
    id: 'rule-mac-cmd-to-linux-ctrl',
    label: 'Mac ⌘ Command → Linux Ctrl（桌面快捷键对齐）',
    targetOs: 'linux',
    sourceKey: 'Command (⌘)',
    targetKey: 'Ctrl',
    enabled: true,
  },
  {
    id: 'rule-mac-opt-to-alt',
    label: 'Mac ⌥ Option → Windows Alt',
    targetOs: 'windows',
    sourceKey: 'Option (⌥)',
    targetKey: 'Alt',
    enabled: true,
  },
  {
    id: 'rule-caps-to-esc',
    label: 'CapsLock → Escape（全局可选）',
    targetOs: 'all',
    sourceKey: 'CapsLock',
    targetKey: 'Escape',
    enabled: false,
  },
];

export const INITIAL_TRANSFERS: TransferRecord[] = [];
