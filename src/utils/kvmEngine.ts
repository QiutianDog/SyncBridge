export type OsKind = 'macos' | 'windows' | 'linux-wayland' | 'linux-x11';

export type OsFamily = 'macos' | 'windows' | 'linux';

export interface GridPos {
  col: number;
  row: number;
}

export interface DeviceNode {
  id: string;
  name: string;
  hostname: string;
  os: OsKind;
  ip: string;
  resolution: { width: number; height: number };
  refreshRateHz: number;
  isPrimary: boolean;
  gridPos: GridPos;
  autoPair: boolean;
  online: boolean;
  latencyMs: number;
  fingerprint: string;
  pairedAt: string;
}

export interface DiscoveredPeer {
  id: string;
  name: string;
  hostname: string;
  os: OsKind;
  ip: string;
  quicPort?: number;
  resolution: { width: number; height: number };
  refreshRateHz: number;
  latencyMs: number;
  fingerprint: string;
}

export interface LocalSystemStatus {
  localNodeId: string;
  localOs: OsKind;
  localIp: string;
  quicPort: number;
  discoveryPort: number;
  fingerprint: string;
  inputPermissionGranted: boolean;
  connectedPeerCount: number;
}

export interface NeighborMap {
  up?: DeviceNode;
  down?: DeviceNode;
  left?: DeviceNode;
  right?: DeviceNode;
}

export interface VirtualCursorState {
  activeNodeId: string;
  /** Normalized [0..1] within active screen */
  x: number;
  /** Normalized [0..1] within active screen */
  y: number;
  lastTransitionEdge?: 'up' | 'down' | 'left' | 'right';
  transitionCount: number;
}

export interface KeyMappingRule {
  id: string;
  label: string;
  targetOs: 'all' | OsFamily;
  sourceKey: string;
  targetKey: string;
  enabled: boolean;
}

export interface TransferRecord {
  id: string;
  type: 'clipboard-text' | 'clipboard-image' | 'file-drop';
  title: string;
  sizeLabel: string;
  fromNodeName: string;
  toNodeName: string;
  timestamp: string;
  encrypted: boolean;
  digestHex?: string;
  status: 'completed' | 'transferring';
}

export type LinuxWaylandStrategy = 'portal-and-uinput-fallback' | 'portal-only';

export interface EngineTuning {
  daemonActive: boolean;
  hidPollingHz: 500 | 1000;
  edgeDelayMs: number;
  releaseHotkey: string;
  linuxWaylandStrategy: LinuxWaylandStrategy;
  /** macOS configuration: hide Dock icon (defaults to false / NOT hidden) */
  hideDockIcon: boolean;
  /** Friendly hostname advertised on LAN discovery */
  localDeviceName: string;
}

export interface ClipboardPolicy {
  syncPlainText: boolean;
  syncImages: boolean;
  enableDragDropFile: boolean;
  maxFileSizeMb: number;
  saveDir: string;
}

export interface FastPointerDatagramFrame {
  frameType: number;
  targetNode: number;
  seq: number;
  timestampUs: number;
  dxSubpx: number;
  dySubpx: number;
}

export const MAX_DEVICES = 9;
export const GRID_COLS = 5;
export const GRID_ROWS = 5;
export const DEFAULT_QUIC_PORT = 24800;
export const DISCOVERY_UDP_PORT = 24801;

export function getOsFamily(os: OsKind): OsFamily {
  if (os === 'macos') return 'macos';
  if (os === 'windows') return 'windows';
  return 'linux';
}

export function getOsLabel(os: OsKind): string {
  switch (os) {
    case 'macos':
      return 'macOS';
    case 'windows':
      return 'Windows';
    case 'linux-wayland':
      return 'Linux · Wayland';
    case 'linux-x11':
      return 'Linux · X11';
  }
}

export function computeNeighbors(nodes: DeviceNode[], nodeId: string): NeighborMap {
  const target = nodes.find((n) => n.id === nodeId);
  if (!target) return {};
  const { col, row } = target.gridPos;

  return {
    up: nodes.find((n) => n.gridPos.col === col && n.gridPos.row === row - 1),
    down: nodes.find((n) => n.gridPos.col === col && n.gridPos.row === row + 1),
    left: nodes.find((n) => n.gridPos.col === col - 1 && n.gridPos.row === row),
    right: nodes.find((n) => n.gridPos.col === col + 1 && n.gridPos.row === row),
  };
}

/**
 * Checks whether all nodes form a single orthogonally connected component
 * (every screen is reachable via Up/Down/Left/Right edge transitions).
 */
export function isTopologyConnected(nodes: DeviceNode[]): boolean {
  if (nodes.length <= 1) return true;

  const visited = new Set<string>();
  const queue: DeviceNode[] = [nodes[0]];
  visited.add(nodes[0].id);

  while (queue.length > 0) {
    const current = queue.shift()!;
    for (const candidate of nodes) {
      if (visited.has(candidate.id)) continue;
      const dc = Math.abs(candidate.gridPos.col - current.gridPos.col);
      const dr = Math.abs(candidate.gridPos.row - current.gridPos.row);
      if (dc + dr === 1) {
        visited.add(candidate.id);
        queue.push(candidate);
      }
    }
  }

  return visited.size === nodes.length;
}

export function setPrimaryNode(nodes: DeviceNode[], primaryId: string): DeviceNode[] {
  if (!nodes.some((n) => n.id === primaryId)) return nodes;
  return nodes.map((node) => ({
    ...node,
    isPrimary: node.id === primaryId,
  }));
}

export function moveNodeToCell(
  nodes: DeviceNode[],
  nodeId: string,
  targetCol: number,
  targetRow: number
): { ok: boolean; nodes: DeviceNode[]; swappedWith?: string; error?: string } {
  if (targetCol < 0 || targetCol >= GRID_COLS || targetRow < 0 || targetRow >= GRID_ROWS) {
    return { ok: false, nodes, error: '目标网格位置超出拓扑画布范围。' };
  }

  const moving = nodes.find((n) => n.id === nodeId);
  if (!moving) {
    return { ok: false, nodes, error: '未找到要移动的屏幕节点。' };
  }

  if (moving.gridPos.col === targetCol && moving.gridPos.row === targetRow) {
    return { ok: true, nodes };
  }

  const occupant = nodes.find(
    (n) => n.gridPos.col === targetCol && n.gridPos.row === targetRow
  );

  let nextNodes: DeviceNode[];
  if (occupant) {
    // Swap positions between the two nodes
    nextNodes = nodes.map((n) => {
      if (n.id === moving.id) {
        return { ...n, gridPos: { col: targetCol, row: targetRow } };
      }
      if (n.id === occupant.id) {
        return { ...n, gridPos: { ...moving.gridPos } };
      }
      return n;
    });
  } else {
    nextNodes = nodes.map((n) =>
      n.id === moving.id ? { ...n, gridPos: { col: targetCol, row: targetRow } } : n
    );
  }

  if (!isTopologyConnected(nextNodes)) {
    return {
      ok: false,
      nodes,
      error: '每台屏幕必须至少与一台其他屏幕保持上、下、左、右相邻，确保鼠标可跨屏移动。',
    };
  }

  return {
    ok: true,
    nodes: nextNodes,
    swappedWith: occupant?.name,
  };
}

export function findNextAdjacentSlot(nodes: DeviceNode[]): GridPos | null {
  if (nodes.length === 0) return { col: 2, row: 2 };
  const occupied = new Set(nodes.map((n) => `${n.gridPos.col},${n.gridPos.row}`));
  const primary = nodes.find((n) => n.isPrimary) ?? nodes[0];
  const orderedAnchors = [primary, ...nodes.filter((n) => n.id !== primary.id)];
  const deltas: GridPos[] = [
    { col: 1, row: 0 },
    { col: -1, row: 0 },
    { col: 0, row: -1 },
    { col: 0, row: 1 },
  ];

  for (const anchor of orderedAnchors) {
    for (const d of deltas) {
      const col = anchor.gridPos.col + d.col;
      const row = anchor.gridPos.row + d.row;
      if (
        col >= 0 &&
        col < GRID_COLS &&
        row >= 0 &&
        row < GRID_ROWS &&
        !occupied.has(`${col},${row}`)
      ) {
        return { col, row };
      }
    }
  }
  return null;
}

export function addPairedDevice(
  nodes: DeviceNode[],
  peer: DiscoveredPeer,
  autoPair: boolean,
  pairedAt = '刚刚'
): { ok: boolean; nodes: DeviceNode[]; addedNode?: DeviceNode; error?: string } {
  if (nodes.length >= MAX_DEVICES) {
    return {
      ok: false,
      nodes,
      error: `最多支持 ${MAX_DEVICES} 台电脑互联，当前已达上限（${MAX_DEVICES}/${MAX_DEVICES}）。`,
    };
  }

  if (nodes.some((n) => n.id === peer.id || n.ip === peer.ip)) {
    return {
      ok: false,
      nodes,
      error: '该局域网设备已在互联拓扑中。',
    };
  }

  const slot = findNextAdjacentSlot(nodes);
  if (!slot) {
    return {
      ok: false,
      nodes,
      error: '拓扑网格无可用相邻空位，请先调整现有屏幕位置。',
    };
  }

  const addedNode: DeviceNode = {
    id: peer.id,
    name: peer.name,
    hostname: peer.hostname,
    os: peer.os,
    ip: peer.ip,
    resolution: peer.resolution,
    refreshRateHz: peer.refreshRateHz,
    isPrimary: nodes.length === 0,
    gridPos: slot,
    autoPair,
    online: true,
    latencyMs: peer.latencyMs,
    fingerprint: peer.fingerprint,
    pairedAt,
  };

  return {
    ok: true,
    nodes: [...nodes, addedNode],
    addedNode,
  };
}

export function stepVirtualCursor(
  nodes: DeviceNode[],
  cursor: VirtualCursorState,
  dx: number,
  dy: number
): VirtualCursorState {
  const activeNode = nodes.find((n) => n.id === cursor.activeNodeId);
  if (!activeNode) {
    const fallback = nodes.find((n) => n.isPrimary) ?? nodes[0];
    return fallback
      ? { activeNodeId: fallback.id, x: 0.5, y: 0.5, transitionCount: cursor.transitionCount }
      : cursor;
  }

  const neighbors = computeNeighbors(nodes, activeNode.id);
  let nextX = cursor.x + dx;
  let nextY = cursor.y + dy;
  let nextNodeId = activeNode.id;
  let edge: VirtualCursorState['lastTransitionEdge'] = cursor.lastTransitionEdge;
  let transitions = cursor.transitionCount;

  if (nextX < 0) {
    if (neighbors.left && neighbors.left.online) {
      nextNodeId = neighbors.left.id;
      nextX = Math.max(0.04, 1 + nextX);
      edge = 'left';
      transitions += 1;
    } else {
      nextX = 0;
    }
  } else if (nextX > 1) {
    if (neighbors.right && neighbors.right.online) {
      nextNodeId = neighbors.right.id;
      nextX = Math.min(0.96, nextX - 1);
      edge = 'right';
      transitions += 1;
    } else {
      nextX = 1;
    }
  }

  // Re-check vertical neighbors from current or newly entered node
  const vNeighbors = computeNeighbors(nodes, nextNodeId);
  if (nextY < 0) {
    if (vNeighbors.up && vNeighbors.up.online) {
      nextNodeId = vNeighbors.up.id;
      nextY = Math.max(0.05, 1 + nextY);
      edge = 'up';
      transitions += 1;
    } else {
      nextY = 0;
    }
  } else if (nextY > 1) {
    if (vNeighbors.down && vNeighbors.down.online) {
      nextNodeId = vNeighbors.down.id;
      nextY = Math.min(0.95, nextY - 1);
      edge = 'down';
      transitions += 1;
    } else {
      nextY = 1;
    }
  }

  return {
    activeNodeId: nextNodeId,
    x: Math.min(1, Math.max(0, nextX)),
    y: Math.min(1, Math.max(0, nextY)),
    lastTransitionEdge: edge,
    transitionCount: transitions,
  };
}

export function validatePairingPin(rawPin: string): {
  valid: boolean;
  normalized: string;
  error?: string;
} {
  const cleaned = rawPin.replace(/[\s-]/g, '');
  if (!cleaned) {
    return { valid: false, normalized: '', error: '请输入目标电脑显示的 6 位局域网临时配对码。' };
  }
  if (!/^\d{6}$/.test(cleaned)) {
    return {
      valid: false,
      normalized: cleaned,
      error: '配对码格式无效：需为 6 位数字（例如 482-915）。',
    };
  }
  return {
    valid: true,
    normalized: `${cleaned.slice(0, 3)}-${cleaned.slice(3)}`,
  };
}

export function formatPinFromSeed(seedNumber: number): string {
  const sixDigits = String(Math.abs(Math.trunc(seedNumber)) % 900000 + 100000);
  return `${sixDigits.slice(0, 3)}-${sixDigits.slice(3)}`;
}

export function validateKeyMappingRule(
  existingRules: KeyMappingRule[],
  candidate: Omit<KeyMappingRule, 'id'>
): { ok: boolean; error?: string } {
  const src = candidate.sourceKey.trim();
  const dst = candidate.targetKey.trim();
  if (!src || !dst) {
    return { ok: false, error: '请选择或输入源按键与目标映射按键。' };
  }
  if (src.toLowerCase() === dst.toLowerCase()) {
    return { ok: false, error: '源按键与目标按键相同，无需创建重复映射。' };
  }
  const conflict = existingRules.find(
    (r) =>
      r.enabled &&
      r.sourceKey.toLowerCase() === src.toLowerCase() &&
      (r.targetOs === 'all' ||
        candidate.targetOs === 'all' ||
        r.targetOs === candidate.targetOs)
  );
  if (conflict) {
    return {
      ok: false,
      error: `与现有规则「${conflict.label}」冲突（同一系统下源按键 ${src} 已映射为 ${conflict.targetKey}）。`,
    };
  }
  return { ok: true };
}

export function resolveMappedKey(
  rules: KeyMappingRule[],
  physicalKey: string,
  targetOs: OsKind
): { outputKey: string; matchedRule?: KeyMappingRule } {
  const family = getOsFamily(targetOs);
  // Specific OS rule takes precedence over 'all'
  const specificMatch = rules.find(
    (r) =>
      r.enabled &&
      r.targetOs === family &&
      r.sourceKey.toLowerCase() === physicalKey.toLowerCase()
  );
  if (specificMatch) {
    return { outputKey: specificMatch.targetKey, matchedRule: specificMatch };
  }

  const globalMatch = rules.find(
    (r) =>
      r.enabled &&
      r.targetOs === 'all' &&
      r.sourceKey.toLowerCase() === physicalKey.toLowerCase()
  );
  if (globalMatch) {
    return { outputKey: globalMatch.targetKey, matchedRule: globalMatch };
  }

  return { outputKey: physicalKey };
}

export function forceReturnCursorToPrimary(
  nodes: DeviceNode[],
  cursor: VirtualCursorState
): VirtualCursorState {
  const primary = nodes.find((n) => n.isPrimary) ?? nodes[0];
  if (!primary) return cursor;
  return {
    activeNodeId: primary.id,
    x: 0.5,
    y: 0.5,
    lastTransitionEdge: undefined,
    transitionCount:
      cursor.activeNodeId === primary.id
        ? cursor.transitionCount
        : cursor.transitionCount + 1,
  };
}

/**
 * Encodes a 14-byte Little-Endian FastPointerDatagram matching `crates/spankvm-proto/src/lib.rs`.
 */
export function encodeFastPointerDatagram(frame: FastPointerDatagramFrame): Uint8Array {
  const buf = new ArrayBuffer(14);
  const view = new DataView(buf);
  view.setUint8(0, frame.frameType & 0xff);
  view.setUint8(1, frame.targetNode & 0xff);
  view.setUint32(2, frame.seq >>> 0, true);
  view.setUint32(6, frame.timestampUs >>> 0, true);
  view.setInt16(10, Math.max(-32768, Math.min(32767, Math.trunc(frame.dxSubpx))), true);
  view.setInt16(12, Math.max(-32768, Math.min(32767, Math.trunc(frame.dySubpx))), true);
  return new Uint8Array(buf);
}

export function decodeFastPointerDatagram(bytes: Uint8Array): FastPointerDatagramFrame | null {
  if (bytes.byteLength < 14) return null;
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return {
    frameType: view.getUint8(0),
    targetNode: view.getUint8(1),
    seq: view.getUint32(2, true),
    timestampUs: view.getUint32(6, true),
    dxSubpx: view.getInt16(10, true),
    dySubpx: view.getInt16(12, true),
  };
}

export function validateLanIpv4(rawIp: string): {
  valid: boolean;
  ip: string;
  port?: number;
  error?: string;
} {
  const trimmed = rawIp.trim();
  if (!trimmed) {
    return { valid: false, ip: '', error: '请输入目标电脑的局域网 IPv4 地址（如 192.168.50.25）。' };
  }

  let host = trimmed;
  let parsedPort: number | undefined;
  if (trimmed.includes(':')) {
    const parts = trimmed.split(':');
    if (parts.length !== 2) {
      return { valid: false, ip: trimmed, error: 'IP 地址格式无效，请使用如 192.168.1.20 或 192.168.1.20:24800。' };
    }
    host = parts[0].trim();
    const p = Number(parts[1]);
    if (!Number.isInteger(p) || p < 1 || p > 65535) {
      return { valid: false, ip: trimmed, error: '端口号需在 1 ~ 65535 之间。' };
    }
    parsedPort = p;
  }

  const segs = host.split('.');
  if (segs.length !== 4 || segs.some((s) => !/^\d{1,3}$/.test(s))) {
    return { valid: false, ip: host, error: '请输入合法的局域网 IPv4 地址（例如 192.168.1.20）。' };
  }
  const nums = segs.map(Number);
  if (nums.some((n) => n < 0 || n > 255)) {
    return { valid: false, ip: host, error: 'IPv4 地址每段数值需在 0 ~ 255 之间。' };
  }

  const isPrivate =
    nums[0] === 10 ||
    nums[0] === 127 ||
    (nums[0] === 172 && nums[1] >= 16 && nums[1] <= 31) ||
    (nums[0] === 192 && nums[1] === 168) ||
    (nums[0] === 169 && nums[1] === 254);

  if (!isPrivate) {
    return {
      valid: false,
      ip: host,
      error: 'SpanKVM 仅限局域网内安全直连，拒绝公网 IP 地址。',
    };
  }

  return { valid: true, ip: host, port: parsedPort };
}

export function encodeArrayBufferToBase64(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer);
  const chunkSize = 0x8000;
  let binary = '';
  for (let i = 0; i < bytes.length; i += chunkSize) {
    const slice = bytes.subarray(i, i + chunkSize);
    binary += String.fromCharCode(...slice);
  }
  if (typeof btoa === 'function') {
    return btoa(binary);
  }
  return Buffer.from(bytes).toString('base64');
}


