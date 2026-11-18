# SpanKVM — 跨系统一套键鼠控多屏软件 (Rust + Tauri v2)

基于 **Rust 高性能核心 + Tauri v2 桌面框架** 构建的跨平台（macOS / Windows / Linux Wayland & X11）键鼠共享软件：
- **一套键鼠控最多 9 台电脑**：5×5 正交连通拓扑编排，自由指定物理键鼠所在的**主屏幕**及各屏幕**上下左右**位置关系；**鼠标移动到哪个屏幕，键盘控制权自动跟随到哪个屏幕**。
- **高刷新率低延迟（60Hz–360Hz）**：基于 `quinn` (QUIC + TLS 1.3)，使用 **RFC 9221 不可靠数据报** 发送 14 字节紧凑鼠标增量帧（零 TCP 队头阻塞，千兆局域网 `< 0.8ms`），独立优先级可靠流传输键盘事件、剪贴板与拖拽文件。
- **纯局域网隔离 & 临时密钥自动配对**：`mdns-sd` 局域网自动发现 + **SPAKE2 6 位一次性临时密钥**首次握手 → 签发双向 `Ed25519` 互信证书并记录指纹，下次同一局域网自动静默加密重连。
- **Linux Wayland 全兼容双栈（确认项 B）**：优先使用现代桌面标准 `xdg-desktop-portal` (`ashpd` InputCapture / RemoteDesktop + `reis` `libei`)，并在不具备 Portal 的合成器上自动回退 `/dev/uinput` + `X11 XI2/XTest`。

## 目录结构

- `crates/spankvm-proto`：14 字节高速指针数据报编解码、可靠流帧协议与跨端共享数据结构
- `crates/spankvm-core`：最多 9 台主机正交连通求解器、跨屏比例坐标路由、防卡键 `KeyUp` 看门狗、按系统按键重映射引擎
- `crates/spankvm-pal`：macOS (`CGEventTap`)、Windows (`Raw Input + SendInput`)、Linux (`Wayland libei + uinput + X11`) 驱动抽象层
- `crates/spankvm-net`：纯局域网 IP 围栏、SPAKE2 临时密钥协商、QUIC 双通道加密传输与 `BLAKE3` 文件块校验
- `src-tauri/`：Tauri v2 原生桌面宿主与后台托盘守护入口
- `src/`：React + TypeScript + Tailwind CSS 控制台前端（同时支持 Tauri v2 原生 IPC 与 Web 预览守护后端）

## 本地桌面构建指南

### 1. 安装前置环境
- Rust Stable (`1.82+`) 与 Node.js (`20+`)
- **Linux (Ubuntu / Debian)**：
  ```bash
  sudo apt update
  sudo apt install -y libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev libudev-dev
  # 可选：为无 InputCapture Portal 的 Wayland 合成器配置免 root uinput 权限
  bash scripts/setup-linux-uinput.sh
  ```

### 2. 启动开发调试或打包安装包
```bash
npm install
cargo check --workspace
npx @tauri-apps/cli@2 dev
# 构建跨平台安装包 (.dmg / .msi / .deb / .AppImage)
npx @tauri-apps/cli@2 build
```
