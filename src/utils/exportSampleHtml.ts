import {
  DeviceNode,
  KeyMappingRule,
  GRID_COLS,
  GRID_ROWS,
  getOsLabel,
  computeNeighbors,
} from './kvmEngine';

function escapeHtml(input: string): string {
  return input
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

export function generateStandaloneSampleHtml(
  nodes: DeviceNode[],
  rules: KeyMappingRule[],
  ephemeralPin: string
): string {
  const primary = nodes.find((n) => n.isPrimary) ?? nodes[0];

  let gridCellsHtml = '';
  for (let r = 0; r < GRID_ROWS; r++) {
    for (let c = 0; c < GRID_COLS; c++) {
      const node = nodes.find((n) => n.gridPos.col === c && n.gridPos.row === r);
      if (node) {
        const nb = computeNeighbors(nodes, node.id);
        const dirs = [
          nb.up ? `上:${nb.up.name}` : '',
          nb.down ? `下:${nb.down.name}` : '',
          nb.left ? `左:${nb.left.name}` : '',
          nb.right ? `右:${nb.right.name}` : '',
        ]
          .filter(Boolean)
          .join(' · ');
        gridCellsHtml += `
          <div class="screen-card ${node.isPrimary ? 'is-primary' : ''}">
            <div class="screen-kicker">${node.isPrimary ? '主屏幕 · 物理键鼠宿主' : '受控从屏 · 焦点自动跟随'}</div>
            <div class="screen-title">${escapeHtml(node.name)}</div>
            <div class="screen-meta">${escapeHtml(getOsLabel(node.os))} · ${node.resolution.width}×${node.resolution.height} · ${node.refreshRateHz}Hz</div>
            <div class="screen-meta mono">${escapeHtml(node.ip)} · ${node.latencyMs.toFixed(1)} ms</div>
            <div class="screen-neighbors">${escapeHtml(dirs || '独立主节点')}</div>
          </div>`;
      } else {
        gridCellsHtml += `<div class="empty-slot">(${c + 1}, ${r + 1}) 空闲拓扑槽位</div>`;
      }
    }
  }

  const rulesRows = rules
    .map(
      (r) => `
      <tr>
        <td>${escapeHtml(r.label)}</td>
        <td>${escapeHtml(r.targetOs === 'all' ? '全部系统' : r.targetOs)}</td>
        <td class="mono">${escapeHtml(r.sourceKey)} → ${escapeHtml(r.targetKey)}</td>
        <td>${r.enabled ? '已启用' : '已停用'}</td>
      </tr>`
    )
    .join('');

  return `<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>SpanKVM — 跨系统键鼠共享软件界面样式示例</title>
  <style>
    :root {
      --bg: #f8fafc;
      --surface: #ffffff;
      --border: #e2e8f0;
      --text: #0f172a;
      --muted: #64748b;
      --accent: #2563eb;
    }
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body {
      font-family: -apple-system, BlinkMacSystemFont, "PingFang SC", "Noto Sans SC", "Segoe UI", sans-serif;
      background: var(--bg);
      color: var(--text);
      line-height: 1.55;
      padding: 32px;
    }
    .container { max-width: 1200px; margin: 0 auto; }
    header {
      display: flex;
      justify-content: space-between;
      align-items: baseline;
      padding-bottom: 20px;
      border-bottom: 1px solid var(--border);
      margin-bottom: 28px;
    }
    h1 { font-size: 22px; font-weight: 700; letter-spacing: -0.02em; }
    h2 { font-size: 16px; font-weight: 600; margin-bottom: 14px; }
    .meta { font-size: 13px; color: var(--muted); }
    .mono { font-family: "JetBrains Mono", ui-monospace, SFMono-Regular, monospace; font-variant-numeric: tabular-nums; }
    .panel {
      background: var(--surface);
      border: 1px solid var(--border);
      border-radius: 12px;
      padding: 24px;
      margin-bottom: 24px;
    }
    .grid-stage {
      display: grid;
      grid-template-columns: repeat(5, minmax(0, 1fr));
      gap: 12px;
    }
    .screen-card {
      border: 1px solid var(--border);
      border-radius: 8px;
      padding: 12px;
      background: #ffffff;
    }
    .screen-card.is-primary {
      border-color: var(--accent);
      background: #eff6ff;
    }
    .screen-kicker { font-size: 11px; color: var(--accent); font-weight: 600; margin-bottom: 4px; }
    .screen-title { font-size: 14px; font-weight: 600; margin-bottom: 4px; }
    .screen-meta { font-size: 12px; color: var(--muted); }
    .screen-neighbors { font-size: 11px; color: #334155; margin-top: 6px; padding-top: 6px; border-top: 1px solid var(--border); }
    .empty-slot {
      border: 1px dashed #cbd5e1;
      border-radius: 8px;
      padding: 12px;
      font-size: 12px;
      color: #94a3b8;
      display: flex;
      align-items: center;
      justify-content: center;
      min-height: 110px;
    }
    table { width: 100%; border-collapse: collapse; font-size: 13px; }
    th, td { text-align: left; padding: 10px 12px; border-bottom: 1px solid var(--border); }
    th { color: var(--muted); font-weight: 500; }
  </style>
</head>
<body>
  <div class="container">
    <header>
      <div>
        <h1>SpanKVM 跨系统键鼠共享配置中心（样式快照）</h1>
        <p class="meta">当前主屏幕：${escapeHtml(primary?.name ?? '未设置')} · 已互联主机：${nodes.length} / 9 台 · 传输加密：QUIC + TLS 1.3 (ChaCha20-Poly1305)</p>
      </div>
      <div class="meta mono">本机局域网临时配对码：${escapeHtml(ephemeralPin)}</div>
    </header>

    <section class="panel">
      <h2>01. 多屏幕上下左右拓扑与主屏幕关系（最多支持 9 台互联）</h2>
      <div class="grid-stage">${gridCellsHtml}</div>
    </section>

    <section class="panel">
      <h2>02. 跨系统键盘按键映射规则</h2>
      <table>
        <thead>
          <tr>
            <th>规则说明</th>
            <th>目标操作系统</th>
            <th>按键映射关系</th>
            <th>状态</th>
          </tr>
        </thead>
        <tbody>${rulesRows}</tbody>
      </table>
    </section>
  </div>
</body>
</html>`;
}
