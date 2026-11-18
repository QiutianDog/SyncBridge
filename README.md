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

## GitHub Actions 云端自动构建与直接下载验证（免装本地编译环境）

仓库已配置 `.github/workflows/build.yml` 跨平台流水线：
1. **自动触发与手动运行**：
   - 向 `main` / `master` / `dev` 分支推送代码时自动触发；也可在 GitHub 仓库顶栏进入 **Actions -> Build SpanKVM Desktop Artifacts -> Run workflow** 手动一键触发。
   - 若推送版本标签（例如 `git tag v0.1.0 && git push --tags`），流水线还会自动创建 **GitHub Release** 并挂载全部安装包。
2. **直接下载四端构建产物（Artifacts）**：
   构建完成后，打开当次 Actions 运行详情页底部的 **Artifacts** 区域即可直接下载对应系统压缩包：
   - `SpanKVM-Windows-x64`：含 `*-setup.exe`、`.msi` 安装包与免安装的 `SpanKVM-portable-x64.exe`
   - `SpanKVM-macOS-Apple-Silicon-arm64`：Apple M1/M2/M3/M4 芯片 `.dmg` 与 `.app.zip`
   - `SpanKVM-macOS-Intel-x64`：Intel 芯片 Mac `.dmg` 与 `.app.zip`
   - `SpanKVM-Linux-Wayland-X11-x64`：含 `.deb`、`.AppImage`、`.rpm`、独立二进制及 `setup-linux-uinput.sh`
3. **首次本地实机运行提示**：
   - **系统托盘与程序坞驻留**：**左键点击系统托盘图标**直接展示并聚焦 SpanKVM 主窗口；**右键点击系统托盘图标**才弹出快捷菜单（暂停共享 / 强制切回本机焦点 / 退出）。macOS 版可在「系统与托盘设置」中开启「隐藏程序坞 (Dock) 图标」（**默认不隐藏**）。
   - **macOS（未配置 Apple 付费开发者证书签名时）**：拖入 `/Applications` 后在终端执行一次 `sudo xattr -rd com.apple.quarantine /Applications/SpanKVM.app`，并在「系统设置 -> 隐私与安全性 -> 辅助功能」勾选 `SpanKVM`。
   - **Windows 10/11**：若出现 SmartScreen 提示，点击「更多信息 -> 仍要运行」，并在防火墙弹窗勾选允许「专用局域网」通信（QUIC `UDP 24800` / 局域网广播发现 `UDP 24801`）。
   - **Linux Wayland / X11**：GNOME 46+ / KDE Plasma 6 / Hyprland 首次启动会弹出 InputCapture Portal 授权；若合成器不支持 Portal，执行随包附带的 `bash setup-linux-uinput.sh` 即可开启免 root `uinput` 驱动。

## 本地桌面构建指南

### 1. 安装前置环境
- Rust Stable (`1.82+`) 与 Node.js (`20+`)
- **Linux (Ubuntu / Debian)**：
  ```bash
  sudo apt update
  sudo apt install -y libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev libudev-dev patchelf libfuse2
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

