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
  validateLanIpv4,
  encodeArrayBufferToBase64,
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
    pairedAt: '本机设备',
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
  const invalidMove = moveNodeToCell(sampleNodes, 'win-rig', 4, 4);
  assert.equal(invalidMove.ok, false);
  assert.equal(isTopologyConnected(sampleNodes), true);

  const validMove = moveNodeToCell(sampleNodes, 'win-rig', 1, 2);
  assert.equal(validMove.ok, true);
  assert.equal(computeNeighbors(validMove.nodes, 'mac-main').left?.id, 'win-rig');

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
  const crossedRight = stepVirtualCursor(sampleNodes, start, 0.18, 0);
  assert.equal(crossedRight.activeNodeId, 'win-rig');
  assert.equal(crossedRight.lastTransitionEdge, 'right');
  assert.equal(crossedRight.transitionCount, 1);

  const clampedLeft = stepVirtualCursor(
    sampleNodes,
    { activeNodeId: 'mac-main', x: 0.05, y: 0.5, transitionCount: 0 },
    -0.2,
    0
  );
  assert.equal(clampedLeft.activeNodeId, 'mac-main');
  assert.equal(clampedLeft.x, 0);
});

test('validatePairingPin and validateLanIpv4 enforce LAN-only SPAKE2 security boundaries', () => {
  assert.equal(validatePairingPin('').valid, false);
  assert.equal(validatePairingPin('12a-456').valid, false);
  const okPin = validatePairingPin('482 915');
  assert.equal(okPin.valid, true);
  assert.equal(okPin.normalized, '482-915');

  const okLan = validateLanIpv4('192.168.50.28:24800');
  assert.equal(okLan.valid, true);
  assert.equal(okLan.ip, '192.168.50.28');
  assert.equal(okLan.port, 24800);

  const okRfc1918 = validateLanIpv4('172.20.14.9');
  assert.equal(okRfc1918.valid, true);
  assert.equal(okRfc1918.ip, '172.20.14.9');

  const publicRejected = validateLanIpv4('8.8.8.8');
  assert.equal(publicRejected.valid, false);

  const sampleBytes = new TextEncoder().encode('SpanKVM-BLAKE3-Payload');
  assert.equal(
    Buffer.from(encodeArrayBufferToBase64(sampleBytes.buffer), 'base64').toString('utf-8'),
    'SpanKVM-BLAKE3-Payload'
  );
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

  const conflictCheck = validateKeyMappingRule(rules, {
    label: 'Duplicate',
    targetOs: 'windows',
    sourceKey: 'Command (⌘)',
    targetKey: 'Alt',
    enabled: true,
  });
  assert.equal(conflictCheck.ok, false);

  const linuxAllowed = validateKeyMappingRule(rules, {
    label: 'Mac Command 映射为 Linux Ctrl',
    targetOs: 'linux',
    sourceKey: 'Command (⌘)',
    targetKey: 'Ctrl',
    enabled: true,
  });
  assert.equal(linuxAllowed.ok, true);

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
  assert.equal(workflowYaml.includes('actions/upload-artifact@v7'), true);
  assert.equal(workflowYaml.includes('workflow_dispatch:'), true);

  const gitignoreContent = fs.readFileSync(path.join(rootDir, '.gitignore'), 'utf-8');
  const gitignoreLines = gitignoreContent.split(/\r?\n/).map((l) => l.trim());
  assert.equal(
    gitignoreLines.includes('data/'),
    false,
    '.gitignore must not contain un-anchored "data/" which ignores src/data/seedData.ts'
  );
  assert.equal(gitignoreLines.includes('/data/'), true);
  assert.equal(
    fs.existsSync(path.join(rootDir, 'src/data/seedData.ts')),
    true,
    'src/data/seedData.ts must exist for frontend and daemon builds'
  );

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

test('normalizeDaemonSnapshot, system tray left/right-click wiring, and macOS Dock visibility defaults are verified', async () => {
  const fs = await import('node:fs');
  const path = await import('node:path');
  const { DEFAULT_TUNING, normalizeDaemonSnapshot } = await import('../services/tauriBridge');

  assert.equal(
    DEFAULT_TUNING.hideDockIcon,
    false,
    'macOS hideDockIcon must default to false (NOT hidden)'
  );

  const partialPayload = {
    nodes: sampleNodes,
    activeNodeId: 'win-rig',
    cursorX: 0.72,
    cursorY: 0.35,
    transitionCount: 3,
    keyRules: [],
    tuning: {
      daemonActive: true,
      hidPollingHz: 1000,
      edgeDelayMs: 0,
      releaseHotkey: 'Ctrl + Alt + Shift + Esc',
      linuxWaylandStrategy: 'portal-and-uinput-fallback',
    },
    localEphemeralPin: '482-915',
    activeDriverName: 'macOS CoreGraphics CGEventTap + CGEventPost',
  };

  const normalized = normalizeDaemonSnapshot(partialPayload);
  assert.equal(normalized.cursor.activeNodeId, 'win-rig');
  assert.equal(normalized.cursor.x, 0.72);
  assert.equal(normalized.cursor.y, 0.35);
  assert.equal(normalized.tuning.hideDockIcon, false);
  assert.equal(normalized.systemStatus.quicPort, 24800);

  const normalizedDockHidden = normalizeDaemonSnapshot({
    ...partialPayload,
    tuning: { ...partialPayload.tuning, hideDockIcon: true },
  });
  assert.equal(normalizedDockHidden.tuning.hideDockIcon, true);

  const rootDir = path.resolve(import.meta.dirname, '../..');

  // Verify Tauri Rust tray left-click show & right-click menu + macOS Dock visibility wiring
  const libRs = fs.readFileSync(path.join(rootDir, 'src-tauri/src/lib.rs'), 'utf-8');
  assert.equal(
    libRs.includes('.show_menu_on_left_click(false)'),
    true,
    'System tray must disable menu on left-click so only right-click opens the menu'
  );
  assert.equal(
    libRs.includes('MouseButton::Left') && libRs.includes('show_and_focus_main_window'),
    true,
    'Left-clicking tray icon must show and focus the main SpanKVM window'
  );
  assert.equal(
    libRs.includes('set_dock_visibility(false)'),
    true,
    'lib.rs must apply macOS Dock visibility when hide_dock_icon is enabled'
  );

  const commandsRs = fs.readFileSync(path.join(rootDir, 'src-tauri/src/commands.rs'), 'utf-8');
  assert.equal(
    commandsRs.includes('hide_dock_icon: false'),
    true,
    'commands.rs default persisted state must keep hide_dock_icon = false'
  );
  assert.equal(
    commandsRs.includes('set_dock_visibility(!hide_dock)'),
    true,
    'commands.rs must dynamically toggle macOS Dock visibility via set_dock_visibility(!hide_dock)'
  );

  // Verify obsolete prototype viewer files were removed
  for (const deadFile of [
    'src/components/WorkspaceSourceExplorer.tsx',
    'src/components/RustArchitectureView.tsx',
    'src/utils/exportSampleHtml.ts',
  ]) {
    assert.equal(
      fs.existsSync(path.join(rootDir, deadFile)),
      false,
      `Obsolete prototype file should be removed: ${deadFile}`
    );
  }
});

test('Windows Win32 0.58 PAL handle types and Linux x11rb Connection trait imports remain regression-free', async () => {
  const fs = await import('node:fs');
  const path = await import('node:path');
  const rootDir = path.resolve(import.meta.dirname, '../..');

  const winRs = fs.readFileSync(path.join(rootDir, 'crates/spankvm-pal/src/windows.rs'), 'utf-8');
  assert.equal(
    winRs.includes('Option::<HHOOK>::None') ||
      winRs.includes('Option::<HINSTANCE>::None') ||
      winRs.includes('Option::<HWND>::None'),
    false,
    'windows.rs must not pass Option::<HHOOK/HINSTANCE/HWND>::None to windows 0.58 CopyType APIs'
  );
  assert.equal(winRs.includes('HHOOK::default()'), true);
  assert.equal(winRs.includes('HINSTANCE::default()'), true);
  assert.equal(winRs.includes('HWND::default()'), true);

  const linuxRs = fs.readFileSync(
    path.join(rootDir, 'crates/spankvm-pal/src/linux_wayland.rs'),
    'utf-8'
  );
  const probeBlock = linuxRs.slice(
    linuxRs.indexOf('pub fn probe_linux_display()'),
    linuxRs.indexOf('pub struct LinuxDualStackDriver')
  );
  assert.equal(
    probeBlock.includes('use x11rb::connection::Connection;'),
    true,
    'probe_linux_display must import x11rb::connection::Connection before calling conn.setup()'
  );

  const workflowYaml = fs.readFileSync(path.join(rootDir, '.github/workflows/build.yml'), 'utf-8');
  assert.equal(workflowYaml.includes('NO_STRIP: true'), true);
  assert.equal(workflowYaml.includes('pkg-config'), true);
  assert.equal(workflowYaml.includes('-p spankvm-pal'), true);
});

test('Exclusive single-machine input suppression and lossless Snipaste/WeChat image clipboard sync are wired end-to-end', async () => {
  const fs = await import('node:fs');
  const path = await import('node:path');
  const rootDir = path.resolve(import.meta.dirname, '../..');

  // 1. macOS: must bypass core-graphics 0.24 broken CGEventTap::new wrapper and return NULL (std::ptr::null_mut())
  const macRs = fs.readFileSync(path.join(rootDir, 'crates/spankvm-pal/src/macos.rs'), 'utf-8');
  assert.equal(
    macRs.includes('CGEventTap::new'),
    false,
    'macos.rs must NOT use core_graphics::event::CGEventTap::new because returning None passes events through'
  );
  assert.equal(macRs.includes('CGEventTapCreate('), true);
  assert.equal(macRs.includes('std::ptr::null_mut()'), true);
  assert.equal(macRs.includes('CGSetLocalEventsSuppressionInterval'), true);
  assert.equal(macRs.includes('pin_cursor_at_park'), true);
  assert.equal(macRs.includes('cancel_warp_delta'), true);
  assert.equal(macRs.includes('debiased.signum() == drift.signum()'), true);
  assert.equal(macRs.includes('pub fn prevent_app_nap()'), true);
  assert.equal(macRs.includes('pthread_set_qos_class_self_np'), true);
  assert.equal(macRs.includes('is_in_edge_cooldown()'), true);
  assert.equal(macRs.includes('set_suppressing_from_hook(true)'), true);

  // 1b. Tray icon & App Nap guard in src-tauri
  const libRs = fs.readFileSync(path.join(rootDir, 'src-tauri/src/lib.rs'), 'utf-8');
  assert.equal(libRs.includes('icons/tray-icon@2x.png'), true);
  assert.equal(libRs.includes('.icon_as_template(cfg!(target_os = "macos"))'), true);
  assert.equal(libRs.includes('prevent_app_nap()'), true);
  assert.equal(fs.existsSync(path.join(rootDir, 'src-tauri/icons/tray-icon.png')), true);
  assert.equal(fs.existsSync(path.join(rootDir, 'src-tauri/icons/tray-icon@2x.png')), true);
  assert.equal(fs.existsSync(path.join(rootDir, 'public/favicon.svg')), true);

  // 2. Windows: Per-Monitor DPI v2 + immediate hook suppression + SetCursorPos warp settle + atomic modifier tracking
  const winRs = fs.readFileSync(path.join(rootDir, 'crates/spankvm-pal/src/windows.rs'), 'utf-8');
  assert.equal(winRs.includes('DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2'), true);
  assert.equal(winRs.includes('set_suppressing_from_hook(true)'), true);
  assert.equal(winRs.includes('warp_settled: AtomicBool'), true);
  assert.equal(winRs.includes('LRESULT(1)'), true);
  assert.equal(winRs.includes('ctrl_held: AtomicBool'), true);

  // 3. Clipboard PAL + Tauri runtime: Snipaste/OS screenshot capture + WeChat multi-format CF_DIB & PNG output
  const clipRs = fs.readFileSync(path.join(rootDir, 'crates/spankvm-pal/src/clipboard.rs'), 'utf-8');
  assert.equal(clipRs.includes('pub fn read_os_clipboard_snapshot('), true);
  assert.equal(clipRs.includes('pub fn write_os_clipboard_png_or_rgba('), true);
  assert.equal(clipRs.includes('CF_DIB_ID'), true);
  assert.equal(clipRs.includes('EmptyClipboard()'), true);
  assert.equal(clipRs.includes('public.png'), true);

  const runtimeRs = fs.readFileSync(path.join(rootDir, 'src-tauri/src/runtime.rs'), 'utf-8');
  assert.equal(runtimeRs.includes('read_os_clipboard_snapshot('), true);
  assert.equal(runtimeRs.includes('ClipboardSnapshot::Image'), true);
  assert.equal(runtimeRs.includes('write_os_clipboard_png_or_rgba('), true);
  assert.equal(runtimeRs.includes('os_clipboard_sequence_token()'), true);
  assert.equal(runtimeRs.includes('last_edge_cross_at'), true);
  assert.equal(runtimeRs.includes('arm_edge_cooldown_ms(120)'), true);
  assert.equal(runtimeRs.includes('bind_lan_udp_listener('), true);

  // 4. macOS Sequoia Local Network Privacy + multi-NIC broadcast bypassing Clash/VPN TUN
  const infoPlistPath = path.join(rootDir, 'src-tauri/Info.plist');
  assert.equal(fs.existsSync(infoPlistPath), true, 'src-tauri/Info.plist must exist for macOS Sequoia');
  const infoPlist = fs.readFileSync(infoPlistPath, 'utf-8');
  assert.equal(infoPlist.includes('NSLocalNetworkUsageDescription'), true);

  const netRs = fs.readFileSync(path.join(rootDir, 'crates/spankvm-net/src/lib.rs'), 'utf-8');
  assert.equal(netRs.includes('enumerate_lan_ipv4_interfaces('), true);
  assert.equal(netRs.includes('bind_lan_udp_listener('), true);
  assert.equal(netRs.includes('is_cgnat_or_bench_ipv4('), true);
  assert.equal(netRs.includes('if_addrs::get_if_addrs()'), true);
});




