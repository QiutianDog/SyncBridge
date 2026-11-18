import test from 'node:test';
import assert from 'node:assert/strict';
import {
  MAX_DEVICES,
  DeviceNode,
  DiscoveredPeer,
  KeyMappingRule,
  computeNeighbors,
  isTopologyConnected,
  setPrimaryNode,
  moveNodeToCell,
  addPairedDevice,
  stepVirtualCursor,
  validatePairingPin,
  validateKeyMappingRule,
  resolveMappedKey,
} from './kvmEngine';

const sampleNodes: DeviceNode[] = [
  {
    id: 'mac-main',
    name: 'MacBook Pro M4',
    hostname: 'macbook-pro.local',
    os: 'macos',
    ip: '192.168.50.10',
    resolution: { width: 3024, height: 1964 },
    refreshRateHz: 120,
    isPrimary: true,
    gridPos: { col: 2, row: 2 },
    autoPair: true,
    online: true,
    latencyMs: 0.2,
    fingerprint: 'ED25519:8F:31:9C:A2',
    pairedAt: '本机主控',
  },
  {
    id: 'win-rig',
    name: 'Win11 Workstation',
    hostname: 'ryzen-tower.local',
    os: 'windows',
    ip: '192.168.50.18',
    resolution: { width: 3840, height: 2160 },
    refreshRateHz: 240,
    isPrimary: false,
    gridPos: { col: 3, row: 2 },
    autoPair: true,
    online: true,
    latencyMs: 0.6,
    fingerprint: 'ED25519:4B:77:D0:19',
    pairedAt: '2026-11-15',
  },
  {
    id: 'linux-dev',
    name: 'Ubuntu Wayland',
    hostname: 'ubuntu-studio.local',
    os: 'linux-wayland',
    ip: '192.168.50.24',
    resolution: { width: 2560, height: 1440 },
    refreshRateHz: 144,
    isPrimary: false,
    gridPos: { col: 2, row: 1 },
    autoPair: true,
    online: true,
    latencyMs: 0.5,
    fingerprint: 'ED25519:1A:9E:55:04',
    pairedAt: '2026-11-16',
  },
];

test('computeNeighbors resolves up, down, left, right screen relationships accurately', () => {
  const macNeighbors = computeNeighbors(sampleNodes, 'mac-main');
  assert.equal(macNeighbors.right?.id, 'win-rig');
  assert.equal(macNeighbors.up?.id, 'linux-dev');
  assert.equal(macNeighbors.left, undefined);
  assert.equal(macNeighbors.down, undefined);
});

test('setPrimaryNode switches primary host exclusively', () => {
  const updated = setPrimaryNode(sampleNodes, 'win-rig');
  assert.equal(updated.find((n) => n.id === 'win-rig')?.isPrimary, true);
  assert.equal(updated.find((n) => n.id === 'mac-main')?.isPrimary, false);
  assert.equal(updated.filter((n) => n.isPrimary).length, 1);
});

test('moveNodeToCell rejects isolated islands and permits valid adjacent or swap moves', () => {
  // Moving win-rig to (4, 4) disconnects it from mac-main (2,2) and linux-dev (2,1)
  const invalidMove = moveNodeToCell(sampleNodes, 'win-rig', 4, 4);
  assert.equal(invalidMove.ok, false);
  assert.equal(isTopologyConnected(sampleNodes), true);

  // Moving win-rig to left of mac-main (1, 2) keeps topology connected
  const validMove = moveNodeToCell(sampleNodes, 'win-rig', 1, 2);
  assert.equal(validMove.ok, true);
  assert.equal(computeNeighbors(validMove.nodes, 'mac-main').left?.id, 'win-rig');

  // Swapping win-rig with mac-main preserves connectivity
  const swapMove = moveNodeToCell(sampleNodes, 'win-rig', 2, 2);
  assert.equal(swapMove.ok, true);
  assert.equal(swapMove.swappedWith, 'MacBook Pro M4');
});

test('addPairedDevice enforces maximum of 9 interconnected computers', () => {
  let current = [...sampleNodes];
  for (let i = current.length; i < MAX_DEVICES; i++) {
    const peer: DiscoveredPeer = {
      id: `node-${i + 1}`,
      name: `Node ${i + 1}`,
      hostname: `node-${i + 1}.local`,
      os: 'linux-x11',
      ip: `192.168.50.${30 + i}`,
      resolution: { width: 1920, height: 1080 },
      refreshRateHz: 144,
      latencyMs: 0.7,
      fingerprint: `ED25519:AA:BB:CC:0${i}`,
    };
    const res = addPairedDevice(current, peer, true);
    assert.equal(res.ok, true);
    current = res.nodes;
  }
  assert.equal(current.length, 9);
  assert.equal(isTopologyConnected(current), true);

  // 10th computer must be rejected
  const tenthPeer: DiscoveredPeer = {
    id: 'node-10',
    name: 'Overflow PC',
    hostname: 'overflow.local',
    os: 'windows',
    ip: '192.168.50.99',
    resolution: { width: 1920, height: 1080 },
    refreshRateHz: 60,
    latencyMs: 0.9,
    fingerprint: 'ED25519:FF:FF:FF:10',
  };
  const rejected = addPairedDevice(current, tenthPeer, true);
  assert.equal(rejected.ok, false);
  assert.equal(rejected.nodes.length, 9);
});

test('stepVirtualCursor crosses shared edges so keyboard focus follows active screen', () => {
  const start = {
    activeNodeId: 'mac-main',
    x: 0.92,
    y: 0.5,
    transitionCount: 0,
  };
  // Push right across mac-main -> win-rig border
  const crossedRight = stepVirtualCursor(sampleNodes, start, 0.18, 0);
  assert.equal(crossedRight.activeNodeId, 'win-rig');
  assert.equal(crossedRight.lastTransitionEdge, 'right');
  assert.equal(crossedRight.transitionCount, 1);

  // Push left on mac-main where no neighbor exists -> clamps at 0 on mac-main
  const clampedLeft = stepVirtualCursor(
    sampleNodes,
    { activeNodeId: 'mac-main', x: 0.05, y: 0.5, transitionCount: 0 },
    -0.2,
    0
  );
  assert.equal(clampedLeft.activeNodeId, 'mac-main');
  assert.equal(clampedLeft.x, 0);
});

test('validatePairingPin validates 6-digit temporary LAN pairing codes', () => {
  assert.equal(validatePairingPin('').valid, false);
  assert.equal(validatePairingPin('12a-456').valid, false);
  const ok = validatePairingPin('482 915');
  assert.equal(ok.valid, true);
  assert.equal(ok.normalized, '482-915');
});

test('key mapping rules validate conflicts and translate per-OS keys accurately', () => {
  const rules: KeyMappingRule[] = [
    {
      id: 'r1',
      label: 'Mac Command 映射为 Windows Ctrl',
      targetOs: 'windows',
      sourceKey: 'Command (⌘)',
      targetKey: 'Ctrl',
      enabled: true,
    },
    {
      id: 'r2',
      label: '全局 CapsLock 映射为 Escape',
      targetOs: 'all',
      sourceKey: 'CapsLock',
      targetKey: 'Escape',
      enabled: true,
    },
  ];

  // Conflict on Windows Command (⌘)
  const conflictCheck = validateKeyMappingRule(rules, {
    label: 'Duplicate',
    targetOs: 'windows',
    sourceKey: 'Command (⌘)',
    targetKey: 'Alt',
    enabled: true,
  });
  assert.equal(conflictCheck.ok, false);

  // Allowed on Linux since r1 only targets Windows
  const linuxAllowed = validateKeyMappingRule(rules, {
    label: 'Mac Command 映射为 Linux Ctrl',
    targetOs: 'linux',
    sourceKey: 'Command (⌘)',
    targetKey: 'Ctrl',
    enabled: true,
  });
  assert.equal(linuxAllowed.ok, true);

  // Resolve key on Windows vs macOS
  assert.equal(resolveMappedKey(rules, 'Command (⌘)', 'windows').outputKey, 'Ctrl');
  assert.equal(resolveMappedKey(rules, 'Command (⌘)', 'macos').outputKey, 'Command (⌘)');
  assert.equal(resolveMappedKey(rules, 'CapsLock', 'linux-wayland').outputKey, 'Escape');
});

test('14-byte FastPointerDatagram codec round-trips accurately and emergency return restores primary focus', async () => {
  const { encodeFastPointerDatagram, decodeFastPointerDatagram, forceReturnCursorToPrimary } =
    await import('./kvmEngine');

  const raw = encodeFastPointerDatagram({
    frameType: 1,
    targetNode: 3,
    seq: 98412,
    timestampUs: 174208123,
    dxSubpx: -148,
    dySubpx: 64,
  });
  assert.equal(raw.byteLength, 14);
  const decoded = decodeFastPointerDatagram(raw);
  assert.deepEqual(decoded, {
    frameType: 1,
    targetNode: 3,
    seq: 98412,
    timestampUs: 174208123,
    dxSubpx: -148,
    dySubpx: 64,
  });

  const reclaimed = forceReturnCursorToPrimary(sampleNodes, {
    activeNodeId: 'win-rig',
    x: 0.85,
    y: 0.2,
    transitionCount: 4,
  });
  assert.equal(reclaimed.activeNodeId, 'mac-main');
  assert.equal(reclaimed.x, 0.5);
  assert.equal(reclaimed.y, 0.5);
  assert.equal(reclaimed.transitionCount, 5);
});

test('GitHub Actions workflow and Tauri v2 bundle icon assets are complete and consistent', async () => {
  const fs = await import('node:fs');
  const path = await import('node:path');
  const rootDir = path.resolve(import.meta.dirname, '../..');

  const workflowPath = path.join(rootDir, '.github/workflows/build.yml');
  assert.equal(fs.existsSync(workflowPath), true, '.github/workflows/build.yml must exist');
  const workflowYaml = fs.readFileSync(workflowPath, 'utf-8');

  for (const requiredTarget of [
    'x86_64-pc-windows-msvc',
    'aarch64-apple-darwin',
    'x86_64-apple-darwin',
    'x86_64-unknown-linux-gnu',
  ]) {
    assert.equal(
      workflowYaml.includes(requiredTarget),
      true,
      `workflow must build target ${requiredTarget}`
    );
  }
  assert.equal(workflowYaml.includes('actions/upload-artifact@v4'), true);
  assert.equal(workflowYaml.includes('workflow_dispatch:'), true);

  const tauriConfPath = path.join(rootDir, 'src-tauri/tauri.conf.json');
  const tauriConf = JSON.parse(fs.readFileSync(tauriConfPath, 'utf-8')) as {
    bundle?: { icon?: string[] };
  };
  const icons = tauriConf.bundle?.icon ?? [];
  assert.equal(icons.length >= 5, true, 'tauri.conf.json must declare bundle icons');

  for (const relIcon of icons) {
    const absIcon = path.join(rootDir, 'src-tauri', relIcon);
    assert.equal(fs.existsSync(absIcon), true, `Missing Tauri bundle icon: ${relIcon}`);
    assert.equal(fs.statSync(absIcon).size > 0, true, `Empty Tauri bundle icon: ${relIcon}`);
  }
});


