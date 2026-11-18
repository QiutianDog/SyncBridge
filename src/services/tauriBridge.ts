import type {
  ClipboardPolicy,
  DeviceNode,
  DiscoveredPeer,
  EngineTuning,
  KeyMappingRule,
  TransferRecord,
  VirtualCursorState,
} from '../utils/kvmEngine';

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

export async function invokeDaemonCommand<
  T extends { ok: boolean; snapshot?: DaemonSnapshot; error?: string; swappedWith?: string }
>(command: string, payload: Record<string, unknown> = {}): Promise<T> {
  if (isTauriNativeRuntime() && window.__TAURI_INTERNALS__) {
    try {
      const nativeRes = await window.__TAURI_INTERNALS__.invoke<DaemonSnapshot>(
        command,
        payload
      );
      return { ok: true, snapshot: nativeRes } as T;
    } catch (err) {
      return { ok: false, error: String(err) } as T;
    }
  }

  const res = await fetch(`/api/ipc/${command}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
  const data = (await res.json()) as T;
  return data;
}

export async function fetchWorkspaceManifest(): Promise<WorkspaceFileEntry[]> {
  const res = await fetch('/api/workspace/files');
  if (!res.ok) return [];
  const data = (await res.json()) as { files: WorkspaceFileEntry[] };
  return data.files;
}

export async function fetchWorkspaceFileContent(
  filePath: string
): Promise<{ path: string; category: string; summary: string; content: string } | null> {
  const res = await fetch(`/api/workspace/file?path=${encodeURIComponent(filePath)}`);
  if (!res.ok) return null;
  return (await res.json()) as {
    path: string;
    category: string;
    summary: string;
    content: string;
  };
}
