import express from 'express';
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import type {
  ClipboardPolicy,
  DeviceNode,
  DiscoveredPeer,
  EngineTuning,
  KeyMappingRule,
  LocalSystemStatus,
  TransferRecord,
  VirtualCursorState,
} from './src/utils/kvmEngine.ts';
import {
  DEFAULT_QUIC_PORT,
  DISCOVERY_UDP_PORT,
  addPairedDevice,
  forceReturnCursorToPrimary,
  formatPinFromSeed,
  moveNodeToCell,
  setPrimaryNode,
  stepVirtualCursor,
  validateKeyMappingRule,
  validateLanIpv4,
  validatePairingPin,
} from './src/utils/kvmEngine.ts';
import {
  INITIAL_DISCOVERED_PEERS,
  INITIAL_KEY_RULES,
  INITIAL_NODES,
  INITIAL_TRANSFERS,
} from './src/data/seedData.ts';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const DATA_DIR = path.resolve(__dirname, 'data');
const STATE_FILE = path.join(DATA_DIR, 'spankvm-daemon-state.json');
const RECEIVE_DIR = path.join(DATA_DIR, 'received-files');

interface PersistedDaemonState {
  localNodeId: string;
  nodes: DeviceNode[];
  discoveredPeers: DiscoveredPeer[];
  cursor: VirtualCursorState;
  tuning: EngineTuning;
  clipboardPolicy: ClipboardPolicy;
  keyRules: KeyMappingRule[];
  transfers: TransferRecord[];
  pinSeed: number;
  inputPermissionGranted: boolean;
}

function createInitialDaemonState(): PersistedDaemonState {
  const localNode = INITIAL_NODES[0];
  return {
    localNodeId: localNode.id,
    nodes: structuredClone(INITIAL_NODES),
    discoveredPeers: structuredClone(INITIAL_DISCOVERED_PEERS),
    cursor: {
      activeNodeId: localNode.id,
      x: 0.5,
      y: 0.5,
      transitionCount: 0,
    },
    tuning: {
      daemonActive: true,
      hidPollingHz: 1000,
      edgeDelayMs: 0,
      releaseHotkey: 'Ctrl + Alt + Shift + Esc',
      linuxWaylandStrategy: 'portal-and-uinput-fallback',
      hideDockIcon: false,
      localDeviceName: localNode.name,
    },
    clipboardPolicy: {
      syncPlainText: true,
      syncImages: true,
      enableDragDropFile: true,
      maxFileSizeMb: 500,
      saveDir: '~/Downloads/SpanKVM',
    },
    keyRules: structuredClone(INITIAL_KEY_RULES),
    transfers: structuredClone(INITIAL_TRANSFERS),
    pinSeed: 482915,
    inputPermissionGranted: true,
  };
}

function loadDaemonState(): PersistedDaemonState {
  try {
    if (fs.existsSync(STATE_FILE)) {
      const raw = fs.readFileSync(STATE_FILE, 'utf-8');
      const parsed = JSON.parse(raw) as Partial<PersistedDaemonState>;
      const fallback = createInitialDaemonState();
      const mergedNodes =
        Array.isArray(parsed.nodes) && parsed.nodes.length > 0
          ? parsed.nodes
          : fallback.nodes;
      const localNodeId =
        parsed.localNodeId && mergedNodes.some((n) => n.id === parsed.localNodeId)
          ? parsed.localNodeId
          : (mergedNodes.find((n) => n.isPrimary)?.id ?? mergedNodes[0].id);

      return {
        ...fallback,
        ...parsed,
        localNodeId,
        nodes: mergedNodes,
        tuning: {
          ...fallback.tuning,
          ...(parsed.tuning ?? {}),
          hideDockIcon:
            typeof parsed.tuning?.hideDockIcon === 'boolean'
              ? parsed.tuning.hideDockIcon
              : false,
          localDeviceName:
            typeof parsed.tuning?.localDeviceName === 'string' &&
            parsed.tuning.localDeviceName.trim()
              ? parsed.tuning.localDeviceName
              : fallback.tuning.localDeviceName,
        },
        clipboardPolicy: {
          ...fallback.clipboardPolicy,
          ...(parsed.clipboardPolicy ?? {}),
        },
      };
    }
  } catch {
    // Fall back cleanly if state file is missing or corrupted
  }
  return createInitialDaemonState();
}

function saveDaemonState(state: PersistedDaemonState): void {
  fs.mkdirSync(DATA_DIR, { recursive: true });
  fs.writeFileSync(STATE_FILE, JSON.stringify(state, null, 2), 'utf-8');
}

function generateEd25519Fingerprint(): string {
  const { publicKey } = crypto.generateKeyPairSync('ed25519');
  const der = publicKey.export({ type: 'spki', format: 'der' });
  const digest = crypto.createHash('sha256').update(der).digest('hex').toUpperCase();
  return `ED25519:${digest.slice(0, 2)}:${digest.slice(2, 4)}:${digest.slice(4, 6)}:${digest.slice(6, 8)}`;
}

function computePayloadDigestHex(buf: Buffer | string): string {
  return crypto.createHash('sha256').update(buf).digest('hex').slice(0, 16);
}

function formatBytesHuman(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

async function startServer() {
  const app = express();
  app.use(express.json({ limit: '64mb' }));

  const state = loadDaemonState();

  const buildSnapshot = () => {
    const localNode =
      state.nodes.find((n) => n.id === state.localNodeId) ??
      state.nodes.find((n) => n.isPrimary) ??
      state.nodes[0];

    const systemStatus: LocalSystemStatus = {
      localNodeId: localNode?.id ?? state.localNodeId,
      localOs: localNode?.os ?? 'macos',
      localIp: localNode?.ip ?? '192.168.50.10',
      quicPort: DEFAULT_QUIC_PORT,
      discoveryPort: DISCOVERY_UDP_PORT,
      fingerprint: localNode?.fingerprint ?? 'ED25519:9A:4C:E2:11',
      inputPermissionGranted: state.inputPermissionGranted,
      connectedPeerCount: state.nodes.filter(
        (n) => n.id !== state.localNodeId && n.online
      ).length,
    };

    const activeDriverName =
      systemStatus.localOs === 'macos'
        ? 'macOS CoreGraphics CGEventTap + CGEventPost'
        : systemStatus.localOs === 'windows'
          ? 'Windows Win32 WH_MOUSE_LL / WH_KEYBOARD_LL + SendInput'
          : state.tuning.linuxWaylandStrategy === 'portal-and-uinput-fallback'
            ? 'Linux Wayland Portal (libei) + /dev/uinput 双栈兼容'
            : 'Linux Wayland XDG InputCapture Portal (libei)';

    return {
      nodes: state.nodes,
      discoveredPeers: state.discoveredPeers.filter(
        (p) => !state.nodes.some((n) => n.id === p.id || n.ip === p.ip)
      ),
      cursor: state.cursor,
      tuning: state.tuning,
      clipboardPolicy: state.clipboardPolicy,
      keyRules: state.keyRules,
      transfers: state.transfers,
      localEphemeralPin: formatPinFromSeed(state.pinSeed),
      activeDriverName,
      systemStatus,
    };
  };

  app.get('/api/daemon/state', (_req, res) => {
    res.json(buildSnapshot());
  });

  app.post('/api/ipc/:command', (req, res) => {
    const { command } = req.params;
    const body = req.body ?? {};

    switch (command) {
      case 'get_daemon_snapshot':
      case 'scan_lan_peers':
      case 'open_receive_dir': {
        res.json({ ok: true, snapshot: buildSnapshot() });
        return;
      }

      case 'request_input_permission': {
        state.inputPermissionGranted = true;
        saveDaemonState(state);
        res.json({ ok: true, snapshot: buildSnapshot() });
        return;
      }

      case 'set_primary_node': {
        state.nodes = setPrimaryNode(state.nodes, String(body.primaryId ?? ''));
        saveDaemonState(state);
        res.json({ ok: true, snapshot: buildSnapshot() });
        return;
      }

      case 'move_node_cell': {
        const result = moveNodeToCell(
          state.nodes,
          String(body.nodeId ?? ''),
          Number(body.col),
          Number(body.row)
        );
        if (!result.ok) {
          res.status(400).json({ ok: false, error: result.error });
          return;
        }
        state.nodes = result.nodes;
        saveDaemonState(state);
        res.json({
          ok: true,
          swappedWith: result.swappedWith,
          snapshot: buildSnapshot(),
        });
        return;
      }

      case 'update_node_refresh_rate': {
        const nodeId = String(body.nodeId ?? '');
        const hz = Number(body.hz);
        state.nodes = state.nodes.map((n) =>
          n.id === nodeId ? { ...n, refreshRateHz: hz } : n
        );
        saveDaemonState(state);
        res.json({ ok: true, snapshot: buildSnapshot() });
        return;
      }

      case 'update_engine_tuning': {
        const patch = (body.tuning ?? {}) as Partial<EngineTuning>;
        state.tuning = {
          ...state.tuning,
          ...patch,
        };
        if (typeof patch.localDeviceName === 'string' && patch.localDeviceName.trim()) {
          const cleanName = patch.localDeviceName.trim();
          state.tuning.localDeviceName = cleanName;
          state.nodes = state.nodes.map((n) =>
            n.id === state.localNodeId ? { ...n, name: cleanName } : n
          );
        }
        saveDaemonState(state);
        res.json({ ok: true, snapshot: buildSnapshot() });
        return;
      }

      case 'step_virtual_cursor': {
        if (state.tuning.daemonActive) {
          state.cursor = stepVirtualCursor(
            state.nodes,
            state.cursor,
            Number(body.dx ?? 0),
            Number(body.dy ?? 0)
          );
          saveDaemonState(state);
        }
        res.json({ ok: true, snapshot: buildSnapshot() });
        return;
      }

      case 'jump_cursor_to_node': {
        const nodeId = String(body.nodeId ?? '');
        if (state.nodes.some((n) => n.id === nodeId)) {
          const x =
            typeof body.x === 'number' ? Math.max(0, Math.min(1, body.x)) : 0.5;
          const y =
            typeof body.y === 'number' ? Math.max(0, Math.min(1, body.y)) : 0.5;
          state.cursor = {
            activeNodeId: nodeId,
            x,
            y,
            transitionCount:
              state.cursor.activeNodeId === nodeId
                ? state.cursor.transitionCount
                : state.cursor.transitionCount + 1,
          };
          saveDaemonState(state);
        }
        res.json({ ok: true, snapshot: buildSnapshot() });
        return;
      }

      case 'emergency_return_primary': {
        state.cursor = forceReturnCursorToPrimary(state.nodes, state.cursor);
        saveDaemonState(state);
        res.json({ ok: true, snapshot: buildSnapshot() });
        return;
      }

      case 'rotate_ephemeral_pin': {
        state.pinSeed += 137911 + Math.floor(Math.random() * 900);
        saveDaemonState(state);
        res.json({ ok: true, snapshot: buildSnapshot() });
        return;
      }

      case 'pair_peer_spake2': {
        const pinCheck = validatePairingPin(String(body.pinInput ?? ''));
        if (!pinCheck.valid) {
          res.status(400).json({ ok: false, error: pinCheck.error });
          return;
        }
        const peerId = String(body.peerId ?? '');
        const peer = state.discoveredPeers.find((p) => p.id === peerId);
        if (!peer) {
          res.status(404).json({ ok: false, error: '未找到待配对的局域网设备。' });
          return;
        }
        const certPeer: DiscoveredPeer = {
          ...peer,
          fingerprint: peer.fingerprint || generateEd25519Fingerprint(),
        };
        const nowTime = new Date().toTimeString().slice(0, 5);
        const added = addPairedDevice(
          state.nodes,
          certPeer,
          Boolean(body.autoPair),
          `今日 ${nowTime}`
        );
        if (!added.ok) {
          res.status(400).json({ ok: false, error: added.error });
          return;
        }
        state.nodes = added.nodes;
        state.discoveredPeers = state.discoveredPeers.filter((p) => p.id !== peer.id);
        saveDaemonState(state);
        res.json({
          ok: true,
          addedNode: added.addedNode,
          snapshot: buildSnapshot(),
        });
        return;
      }

      case 'pair_by_ip_spake2': {
        const ipCheck = validateLanIpv4(String(body.ip ?? ''));
        if (!ipCheck.valid) {
          res.status(400).json({ ok: false, error: ipCheck.error });
          return;
        }
        const pinCheck = validatePairingPin(String(body.pinInput ?? ''));
        if (!pinCheck.valid) {
          res.status(400).json({ ok: false, error: pinCheck.error });
          return;
        }

        const existingDiscovered = state.discoveredPeers.find(
          (p) => p.ip === ipCheck.ip
        );
        const manualPeer: DiscoveredPeer = existingDiscovered ?? {
          id: `node-lan-${ipCheck.ip.replace(/\./g, '-')}`,
          name: `LAN-${ipCheck.ip.split('.').slice(-1)[0]}`,
          hostname: `host-${ipCheck.ip.replace(/\./g, '-')}.local`,
          os: 'windows',
          ip: ipCheck.ip,
          quicPort: Number(body.port) || ipCheck.port || DEFAULT_QUIC_PORT,
          resolution: { width: 2560, height: 1440 },
          refreshRateHz: 120,
          latencyMs: 0.6,
          fingerprint: generateEd25519Fingerprint(),
        };

        const nowTime = new Date().toTimeString().slice(0, 5);
        const added = addPairedDevice(
          state.nodes,
          manualPeer,
          Boolean(body.autoPair),
          `今日 ${nowTime}`
        );
        if (!added.ok) {
          res.status(400).json({ ok: false, error: added.error });
          return;
        }
        state.nodes = added.nodes;
        state.discoveredPeers = state.discoveredPeers.filter(
          (p) => p.id !== manualPeer.id && p.ip !== manualPeer.ip
        );
        saveDaemonState(state);
        res.json({
          ok: true,
          addedNode: added.addedNode,
          snapshot: buildSnapshot(),
        });
        return;
      }

      case 'toggle_auto_pair': {
        const nodeId = String(body.nodeId ?? '');
        state.nodes = state.nodes.map((n) =>
          n.id === nodeId ? { ...n, autoPair: !n.autoPair } : n
        );
        saveDaemonState(state);
        res.json({ ok: true, snapshot: buildSnapshot() });
        return;
      }

      case 'remove_paired_node': {
        const nodeId = String(body.nodeId ?? '');
        const target = state.nodes.find((n) => n.id === nodeId);
        if (target && target.id !== state.localNodeId && !target.isPrimary) {
          state.nodes = state.nodes.filter((n) => n.id !== nodeId);
          state.discoveredPeers = [
            {
              id: target.id,
              name: target.name,
              hostname: target.hostname,
              os: target.os,
              ip: target.ip,
              quicPort: DEFAULT_QUIC_PORT,
              resolution: target.resolution,
              refreshRateHz: target.refreshRateHz,
              latencyMs: target.latencyMs,
              fingerprint: target.fingerprint,
            },
            ...state.discoveredPeers.filter((p) => p.id !== target.id),
          ];
          if (state.cursor.activeNodeId === nodeId && state.nodes[0]) {
            state.cursor = {
              activeNodeId: state.nodes[0].id,
              x: 0.5,
              y: 0.5,
              transitionCount: state.cursor.transitionCount,
            };
          }
          saveDaemonState(state);
        }
        res.json({ ok: true, snapshot: buildSnapshot() });
        return;
      }

      case 'add_key_rule': {
        const candidate = body.rule as Omit<KeyMappingRule, 'id'>;
        const check = validateKeyMappingRule(state.keyRules, candidate);
        if (!check.ok) {
          res.status(400).json({ ok: false, error: check.error });
          return;
        }
        state.keyRules = [
          { ...candidate, id: `rule-${Date.now()}` },
          ...state.keyRules,
        ];
        saveDaemonState(state);
        res.json({ ok: true, snapshot: buildSnapshot() });
        return;
      }

      case 'toggle_key_rule': {
        const ruleId = String(body.ruleId ?? '');
        state.keyRules = state.keyRules.map((r) =>
          r.id === ruleId ? { ...r, enabled: !r.enabled } : r
        );
        saveDaemonState(state);
        res.json({ ok: true, snapshot: buildSnapshot() });
        return;
      }

      case 'delete_key_rule': {
        const ruleId = String(body.ruleId ?? '');
        state.keyRules = state.keyRules.filter((r) => r.id !== ruleId);
        saveDaemonState(state);
        res.json({ ok: true, snapshot: buildSnapshot() });
        return;
      }

      case 'update_clipboard_policy': {
        state.clipboardPolicy = {
          ...state.clipboardPolicy,
          ...(body.policy ?? {}),
        };
        saveDaemonState(state);
        res.json({ ok: true, snapshot: buildSnapshot() });
        return;
      }

      case 'broadcast_clipboard_text': {
        const text = String(body.text ?? '').trim();
        if (!text) {
          res.status(400).json({ ok: false, error: '剪贴板文本内容不能为空。' });
          return;
        }
        if (!state.clipboardPolicy.syncPlainText) {
          res
            .status(400)
            .json({ ok: false, error: '纯文本剪贴板同步开关当前已关闭。' });
          return;
        }
        const byteLen = Buffer.byteLength(text, 'utf-8');
        const digestHex = computePayloadDigestHex(text);
        const peerCount = state.nodes.filter(
          (n) => n.id !== state.localNodeId && n.online
        ).length;
        const newRec: TransferRecord = {
          id: `tr-${Date.now()}`,
          type: 'clipboard-text',
          title: text.length > 48 ? `${text.slice(0, 48)}...` : text,
          sizeLabel: formatBytesHuman(byteLen),
          fromNodeName: state.tuning.localDeviceName,
          toNodeName:
            peerCount > 0
              ? `已同步至 ${peerCount} 台在线设备`
              : '本机系统剪贴板',
          timestamp: new Date().toTimeString().slice(0, 8),
          encrypted: true,
          digestHex,
          status: 'completed',
        };
        state.transfers = [newRec, ...state.transfers];
        saveDaemonState(state);
        res.json({ ok: true, snapshot: buildSnapshot() });
        return;
      }

      case 'send_file_payload': {
        if (!state.clipboardPolicy.enableDragDropFile) {
          res.status(400).json({
            ok: false,
            error: '文件拖拽传输开关当前已关闭，请先开启后再发送。',
          });
          return;
        }
        const fileName = path.basename(String(body.fileName ?? 'transfer.bin'));
        const targetNodeId = String(body.targetNodeId ?? '');
        const base64Data = String(body.base64Data ?? '');
        const rawBytes = Buffer.from(base64Data, 'base64');

        const maxBytes = state.clipboardPolicy.maxFileSizeMb * 1024 * 1024;
        if (rawBytes.byteLength > maxBytes) {
          res.status(400).json({
            ok: false,
            error: `文件大小超过当前设置的 ${state.clipboardPolicy.maxFileSizeMb} MB 上限。`,
          });
          return;
        }

        fs.mkdirSync(RECEIVE_DIR, { recursive: true });
        fs.writeFileSync(path.join(RECEIVE_DIR, fileName), rawBytes);

        const targetNode = state.nodes.find((n) => n.id === targetNodeId);
        const digestHex = computePayloadDigestHex(rawBytes);
        const newRec: TransferRecord = {
          id: `tr-${Date.now()}`,
          type: 'file-drop',
          title: fileName,
          sizeLabel: formatBytesHuman(rawBytes.byteLength),
          fromNodeName: state.tuning.localDeviceName,
          toNodeName: targetNode?.name ?? state.tuning.localDeviceName,
          timestamp: new Date().toTimeString().slice(0, 8),
          encrypted: true,
          digestHex,
          status: 'completed',
        };
        state.transfers = [newRec, ...state.transfers];
        saveDaemonState(state);
        res.json({ ok: true, snapshot: buildSnapshot() });
        return;
      }

      case 'clear_transfer_records': {
        state.transfers = [];
        saveDaemonState(state);
        res.json({ ok: true, snapshot: buildSnapshot() });
        return;
      }

      default: {
        res.status(404).json({ ok: false, error: `未知桌面守护命令: ${command}` });
      }
    }
  });

  if (process.env.NODE_ENV !== 'production') {
    const { createServer: createViteServer } = await import('vite');
    const vite = await createViteServer({
      server: { middlewareMode: true },
      appType: 'spa',
    });
    app.use(vite.middlewares);
  } else {
    const distPath = path.resolve(__dirname, 'dist');
    app.use(express.static(distPath));
    app.get('*', (_req, res) => {
      res.sendFile(path.join(distPath, 'index.html'));
    });
  }

  const port = Number(process.env.PORT) || 3000;
  app.listen(port, '0.0.0.0', () => {
    console.log(`SpanKVM daemon server listening on http://0.0.0.0:${port}`);
  });
}

startServer();
