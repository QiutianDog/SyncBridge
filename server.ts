import express from 'express';
import crypto from 'node:crypto';
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import type {
  ClipboardPolicy,
  DeviceNode,
  DiscoveredPeer,
  EngineTuning,
  KeyMappingRule,
  TransferRecord,
  VirtualCursorState,
} from './src/utils/kvmEngine.ts';
import {
  addPairedDevice,
  forceReturnCursorToPrimary,
  formatPinFromSeed,
  moveNodeToCell,
  setPrimaryNode,
  stepVirtualCursor,
  validateKeyMappingRule,
  validatePairingPin,
} from './src/utils/kvmEngine.ts';
import {
  INITIAL_DISCOVERED_PEERS,
  INITIAL_KEY_RULES,
  INITIAL_NODES,
  INITIAL_TRANSFERS,
  NINE_GRID_PRESET,
} from './src/data/seedData.ts';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const DATA_DIR = path.resolve(__dirname, 'data');
const STATE_FILE = path.join(DATA_DIR, 'spankvm-daemon-state.json');

interface PersistedDaemonState {
  nodes: DeviceNode[];
  discoveredPeers: DiscoveredPeer[];
  cursor: VirtualCursorState;
  tuning: EngineTuning;
  clipboardPolicy: ClipboardPolicy;
  keyRules: KeyMappingRule[];
  transfers: TransferRecord[];
  pinSeed: number;
  pinIssuedAtMs: number;
}

function createInitialDaemonState(): PersistedDaemonState {
  return {
    nodes: structuredClone(INITIAL_NODES),
    discoveredPeers: structuredClone(INITIAL_DISCOVERED_PEERS),
    cursor: {
      activeNodeId: INITIAL_NODES[0].id,
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
    pinIssuedAtMs: Date.now(),
  };
}

function loadDaemonState(): PersistedDaemonState {
  try {
    if (fs.existsSync(STATE_FILE)) {
      const raw = fs.readFileSync(STATE_FILE, 'utf-8');
      const parsed = JSON.parse(raw) as Partial<PersistedDaemonState>;
      const fallback = createInitialDaemonState();
      return {
        ...fallback,
        ...parsed,
        tuning: { ...fallback.tuning, ...(parsed.tuning ?? {}) },
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

function computePayloadDigestHex(content: string): string {
  return crypto.createHash('sha256').update(content).digest('hex').slice(0, 16);
}

const WORKSPACE_FILES: { path: string; category: string; summary: string }[] = [
  { path: '.github/workflows/build.yml', category: 'GitHub Actions CI/CD', summary: 'Windows / macOS (ARM64 + Intel) / Linux 四端矩阵自动构建与 Artifacts 产物直出' },
  { path: 'Cargo.toml', category: 'Rust Workspace', summary: 'Cargo Workspace 根清单与统一依赖锁' },
  { path: 'crates/spankvm-proto/src/lib.rs', category: 'spankvm-proto', summary: '14B 高速指针数据报编解码与跨端共享协议结构' },
  { path: 'crates/spankvm-core/src/lib.rs', category: 'spankvm-core', summary: '最多 9 屏正交连通求解、跨屏焦点跟随、防粘连看门狗与键位映射' },
  { path: 'crates/spankvm-pal/src/lib.rs', category: 'spankvm-pal', summary: '跨平台输入捕获与低延迟注入抽象层 Trait' },
  { path: 'crates/spankvm-pal/src/linux_wayland.rs', category: 'spankvm-pal (Wayland)', summary: '确认项 B：Wayland Portal (ashpd + libei) + /dev/uinput + X11 双栈驱动' },
  { path: 'crates/spankvm-pal/src/macos.rs', category: 'spankvm-pal (macOS)', summary: 'macOS CoreGraphics CGEventTap 捕获冻结与 CGEventPost 注入' },
  { path: 'crates/spankvm-pal/src/windows.rs', category: 'spankvm-pal (Windows)', summary: 'Windows RawInput 1000Hz 无加速增量捕获与 SendInput 扫描码注入' },
  { path: 'crates/spankvm-net/src/lib.rs', category: 'spankvm-net', summary: '纯局域网围栏、SPAKE2 6 位临时码协商与 Quinn QUIC 双通道传输' },
  { path: 'src-tauri/tauri.conf.json', category: 'Tauri v2', summary: '确认项 A：Tauri v2 桌面窗口、打包配置与安全策略' },
  { path: 'src-tauri/Cargo.toml', category: 'Tauri v2', summary: 'Tauri v2 宿主依赖配置与系统托盘特性' },
  { path: 'src-tauri/src/lib.rs', category: 'Tauri v2', summary: 'Tauri v2 应用生命周期与关闭窗口转入后台托盘守护' },
  { path: 'src-tauri/src/commands.rs', category: 'Tauri v2', summary: 'Tauri v2 #[tauri::command] 前端 IPC 桥接处理器' },
  { path: 'scripts/setup-linux-uinput.sh', category: 'Linux 辅助脚本', summary: 'Linux Wayland 后备 /dev/uinput 免 root udev 权限一键配置脚本' },
  { path: 'src/data/seedData.ts', category: '前端拓扑预设数据', summary: '默认 4 屏 / 9 屏九宫格拓扑预设与按键映射种子配置' },
  { path: 'README.md', category: '构建文档', summary: 'macOS / Windows / Linux Wayland 本地编译与打包说明' },
];

async function startServer() {
  const app = express();
  app.use(express.json({ limit: '2mb' }));

  let state = loadDaemonState();

  const buildSnapshot = () => ({
    nodes: state.nodes,
    discoveredPeers: state.discoveredPeers,
    cursor: state.cursor,
    tuning: state.tuning,
    clipboardPolicy: state.clipboardPolicy,
    keyRules: state.keyRules,
    transfers: state.transfers,
    localEphemeralPin: formatPinFromSeed(state.pinSeed),
    activeDriverName:
      state.tuning.linuxWaylandStrategy === 'portal-and-uinput-fallback'
        ? 'Wayland Portal (libei) + /dev/uinput 双栈就绪'
        : 'Wayland XDG InputCapture Portal (libei)',
  });

  app.get('/api/daemon/state', (_req, res) => {
    res.json(buildSnapshot());
  });

  app.post('/api/ipc/:command', (req, res) => {
    const { command } = req.params;
    const body = req.body ?? {};

    switch (command) {
      case 'get_daemon_snapshot': {
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
        const hz = Number(body.hz) as DeviceNode['refreshRateHz'];
        state.nodes = state.nodes.map((n) =>
          n.id === nodeId ? { ...n, refreshRateHz: hz } : n
        );
        saveDaemonState(state);
        res.json({ ok: true, snapshot: buildSnapshot() });
        return;
      }

      case 'update_engine_tuning': {
        state.tuning = {
          ...state.tuning,
          ...(body.tuning ?? {}),
        };
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

      case 'load_nine_preset': {
        state.nodes = structuredClone(NINE_GRID_PRESET);
        state.discoveredPeers = INITIAL_DISCOVERED_PEERS.filter(
          (p: DiscoveredPeer) => !NINE_GRID_PRESET.some((n: DeviceNode) => n.id === p.id)
        );
        state.cursor = {
          activeNodeId: NINE_GRID_PRESET[0].id,
          x: 0.5,
          y: 0.5,
          transitionCount: 0,
        };
        saveDaemonState(state);
        res.json({ ok: true, snapshot: buildSnapshot() });
        return;
      }

      case 'reset_default_preset': {
        state = createInitialDaemonState();
        saveDaemonState(state);
        res.json({ ok: true, snapshot: buildSnapshot() });
        return;
      }

      case 'rotate_ephemeral_pin': {
        state.pinSeed += 137911 + Math.floor(Math.random() * 900);
        state.pinIssuedAtMs = Date.now();
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
          fingerprint: generateEd25519Fingerprint(),
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
        if (target && !target.isPrimary) {
          state.nodes = state.nodes.filter((n) => n.id !== nodeId);
          state.discoveredPeers = [
            {
              id: target.id,
              name: target.name,
              hostname: target.hostname,
              os: target.os,
              ip: target.ip,
              resolution: target.resolution,
              refreshRateHz: target.refreshRateHz,
              latencyMs: target.latencyMs,
              fingerprint: target.fingerprint,
            },
            ...state.discoveredPeers,
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

      case 'push_transfer_record': {
        const rec = body.record as Omit<TransferRecord, 'id' | 'timestamp' | 'digestHex'>;
        const digestHex = computePayloadDigestHex(`${rec.title}:${rec.sizeLabel}:${Date.now()}`);
        const newRec: TransferRecord = {
          ...rec,
          id: `tr-${Date.now()}`,
          timestamp: new Date().toTimeString().slice(0, 8),
          digestHex,
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
        res.status(404).json({ ok: false, error: `未知 IPC 命令: ${command}` });
      }
    }
  });

  app.get('/api/workspace/files', (_req, res) => {
    const items = WORKSPACE_FILES.map((entry) => {
      const absPath = path.resolve(__dirname, entry.path);
      let lineCount = 0;
      let byteSize = 0;
      if (fs.existsSync(absPath)) {
        const content = fs.readFileSync(absPath, 'utf-8');
        byteSize = Buffer.byteLength(content, 'utf-8');
        lineCount = content.split('\n').length;
      }
      return { ...entry, lineCount, byteSize };
    });
    res.json({ files: items });
  });

  app.get('/api/workspace/file', (req, res) => {
    const relPath = String(req.query.path ?? '');
    const allowed = WORKSPACE_FILES.find((f) => f.path === relPath);
    if (!allowed) {
      res.status(403).json({ error: '不允许读取未登记路径' });
      return;
    }
    const absPath = path.resolve(__dirname, allowed.path);
    if (!fs.existsSync(absPath)) {
      res.status(404).json({ error: '文件不存在' });
      return;
    }
    const content = fs.readFileSync(absPath, 'utf-8');
    res.json({ path: allowed.path, category: allowed.category, summary: allowed.summary, content });
  });

  app.get('/api/workspace/bundle.tar.gz', (_req, res) => {
    res.setHeader('Content-Type', 'application/gzip');
    res.setHeader(
      'Content-Disposition',
      'attachment; filename="spankvm-rust-tauri-v2-workspace.tar.gz"'
    );
    const tarTargets = [
      '.github',
      'Cargo.toml',
      'crates',
      'src-tauri',
      'scripts',
      'src',
      'index.html',
      'package.json',
      'tsconfig.json',
      'vite.config.ts',
      'README.md',
    ].filter((p) => fs.existsSync(path.resolve(__dirname, p)));

    const child = spawn('tar', ['-czf', '-', ...tarTargets], { cwd: __dirname });
    child.stdout.pipe(res);
    child.on('error', () => {
      if (!res.headersSent) {
        res.status(500).end();
      }
    });
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
    console.log(`SpanKVM daemon bridge & UI server listening on http://0.0.0.0:${port}`);
  });
}

startServer();
