import type {
  ClipboardPolicy,
  DeviceNode,
  DiscoveredPeer,
  EngineTuning,
  KeyMappingRule,
  TransferRecord,
  VirtualCursorState,
} from '../utils/kvmEngine';
import {
  INITIAL_DISCOVERED_PEERS,
  INITIAL_KEY_RULES,
  INITIAL_NODES,
  INITIAL_TRANSFERS,
} from '../data/seedData';

export interface DaemonSnapshot {
  nodes: DeviceNode[];
  discoveredPeers: DiscoveredPeer[];
  cursor: VirtualCursorState;
  tuning: EngineTuning;
  clipboardPolicy: ClipboardPolicy;
  keyRules: KeyMappingRule[];
  transfers: TransferRecord[];
  localEphemeralPin: string;
  activeDriverName: string;
}

export interface WorkspaceFileEntry {
  path: string;
  category: string;
  summary: string;
  lineCount: number;
  byteSize: number;
}

declare global {
  interface Window {
    __TAURI_INTERNALS__?: {
      invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
    };
  }
}

export function isTauriNativeRuntime(): boolean {
  return typeof window !== 'undefined' && Boolean(window.__TAURI_INTERNALS__?.invoke);
}

const DEFAULT_TUNING: EngineTuning = {
  daemonActive: true,
  hidPollingHz: 1000,
  edgeDelayMs: 0,
  releaseHotkey: 'Ctrl + Alt + Shift + Esc',
  linuxWaylandStrategy: 'portal-and-uinput-fallback',
};

const DEFAULT_CLIPBOARD_POLICY: ClipboardPolicy = {
  syncPlainText: true,
  syncImages: true,
  enableDragDropFile: true,
  maxFileSizeMb: 500,
  saveDir: '~/Downloads/SpanKVM',
};

/**
 * Normalizes any daemon snapshot payload (from Tauri v2 Rust IPC or Express HTTP bridge)
 * so missing or legacy fields can never become `undefined` and crash React rendering.
 */
export function normalizeDaemonSnapshot(
  raw: unknown,
  fallback?: Partial<DaemonSnapshot>
): DaemonSnapshot {
  const src = (raw && typeof raw === 'object' ? raw : {}) as Record<string, unknown>;

  const nodes =
    Array.isArray(src.nodes) && src.nodes.length > 0
      ? (src.nodes as DeviceNode[])
      : (fallback?.nodes ?? INITIAL_NODES);

  const primaryId = nodes.find((n) => n.isPrimary)?.id ?? nodes[0]?.id ?? 'node-mac-studio';

  const rawCursor =
    src.cursor && typeof src.cursor === 'object'
      ? (src.cursor as Record<string, unknown>)
      : null;

  const cursor: VirtualCursorState = {
    activeNodeId:
      typeof rawCursor?.activeNodeId === 'string'
        ? rawCursor.activeNodeId
        : typeof src.activeNodeId === 'string'
          ? src.activeNodeId
          : (fallback?.cursor?.activeNodeId ?? primaryId),
    x:
      typeof rawCursor?.x === 'number'
        ? rawCursor.x
        : typeof src.cursorX === 'number'
          ? src.cursorX
          : (fallback?.cursor?.x ?? 0.5),
    y:
      typeof rawCursor?.y === 'number'
        ? rawCursor.y
        : typeof src.cursorY === 'number'
          ? src.cursorY
          : (fallback?.cursor?.y ?? 0.5),
    transitionCount:
      typeof rawCursor?.transitionCount === 'number'
        ? rawCursor.transitionCount
        : typeof src.transitionCount === 'number'
          ? src.transitionCount
          : (fallback?.cursor?.transitionCount ?? 0),
    lastTransitionEdge:
      rawCursor?.lastTransitionEdge === 'up' ||
      rawCursor?.lastTransitionEdge === 'down' ||
      rawCursor?.lastTransitionEdge === 'left' ||
      rawCursor?.lastTransitionEdge === 'right'
        ? rawCursor.lastTransitionEdge
        : undefined,
  };

  const discoveredPeers = Array.isArray(src.discoveredPeers)
    ? (src.discoveredPeers as DiscoveredPeer[])
    : (fallback?.discoveredPeers ??
      INITIAL_DISCOVERED_PEERS.filter((p) => !nodes.some((n) => n.id === p.id)));

  const keyRules = Array.isArray(src.keyRules)
    ? (src.keyRules as KeyMappingRule[])
    : (fallback?.keyRules ?? INITIAL_KEY_RULES);

  const transfers = Array.isArray(src.transfers)
    ? (src.transfers as TransferRecord[])
    : (fallback?.transfers ?? INITIAL_TRANSFERS);

  const rawTuning =
    src.tuning && typeof src.tuning === 'object'
      ? (src.tuning as Partial<EngineTuning>)
      : {};
  const tuning: EngineTuning = {
    ...(fallback?.tuning ?? DEFAULT_TUNING),
    ...rawTuning,
  };

  const rawPolicy =
    src.clipboardPolicy && typeof src.clipboardPolicy === 'object'
      ? (src.clipboardPolicy as Partial<ClipboardPolicy>)
      : {};
  const clipboardPolicy: ClipboardPolicy = {
    ...(fallback?.clipboardPolicy ?? DEFAULT_CLIPBOARD_POLICY),
    ...rawPolicy,
  };

  const localEphemeralPin =
    typeof src.localEphemeralPin === 'string' && src.localEphemeralPin.trim()
      ? src.localEphemeralPin
      : (fallback?.localEphemeralPin ?? '482-915');

  const activeDriverName =
    typeof src.activeDriverName === 'string' && src.activeDriverName.trim()
      ? src.activeDriverName
      : (fallback?.activeDriverName ?? 'Tauri v2 原生跨平台驱动就绪');

  return {
    nodes,
    discoveredPeers,
    cursor,
    tuning,
    clipboardPolicy,
    keyRules,
    transfers,
    localEphemeralPin,
    activeDriverName,
  };
}

export async function invokeDaemonCommand<
  T extends {
    ok: boolean;
    snapshot?: DaemonSnapshot;
    error?: string;
    swappedWith?: string;
    addedNode?: DeviceNode;
  }
>(command: string, payload: Record<string, unknown> = {}): Promise<T> {
  if (isTauriNativeRuntime() && window.__TAURI_INTERNALS__) {
    try {
      const rawEnvelope = await window.__TAURI_INTERNALS__.invoke<Record<string, unknown>>(
        'dispatch_daemon_ipc',
        { command, payload }
      );

      if (rawEnvelope && typeof rawEnvelope === 'object' && 'ok' in rawEnvelope) {
        const normalizedSnapshot = rawEnvelope.snapshot
          ? normalizeDaemonSnapshot(rawEnvelope.snapshot)
          : undefined;
        return {
          ...rawEnvelope,
          snapshot: normalizedSnapshot,
        } as T;
      }

      return {
        ok: true,
        snapshot: normalizeDaemonSnapshot(rawEnvelope),
      } as T;
    } catch (err) {
      return { ok: false, error: String(err) } as T;
    }
  }

  try {
    const res = await fetch(`/api/ipc/${command}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });
    const data = (await res.json()) as T;
    if (data.snapshot) {
      data.snapshot = normalizeDaemonSnapshot(data.snapshot);
    }
    return data;
  } catch (err) {
    return { ok: false, error: String(err) } as T;
  }
}

export async function fetchWorkspaceManifest(): Promise<WorkspaceFileEntry[]> {
  if (isTauriNativeRuntime() && window.__TAURI_INTERNALS__) {
    try {
      const files = await window.__TAURI_INTERNALS__.invoke<WorkspaceFileEntry[]>(
        'get_workspace_files'
      );
      return Array.isArray(files) ? files : [];
    } catch {
      return [];
    }
  }

  try {
    const res = await fetch('/api/workspace/files');
    if (!res.ok) return [];
    const data = (await res.json()) as { files: WorkspaceFileEntry[] };
    return Array.isArray(data.files) ? data.files : [];
  } catch {
    return [];
  }
}

export async function fetchWorkspaceFileContent(
  filePath: string
): Promise<{ path: string; category: string; summary: string; content: string } | null> {
  if (isTauriNativeRuntime() && window.__TAURI_INTERNALS__) {
    try {
      const detail = await window.__TAURI_INTERNALS__.invoke<{
        path: string;
        category: string;
        summary: string;
        content: string;
      } | null>('get_workspace_file', { filePath });
      return detail ?? null;
    } catch {
      return null;
    }
  }

  try {
    const res = await fetch(`/api/workspace/file?path=${encodeURIComponent(filePath)}`);
    if (!res.ok) return null;
    return (await res.json()) as {
      path: string;
      category: string;
      summary: string;
      content: string;
    };
  } catch {
    return null;
  }
}
