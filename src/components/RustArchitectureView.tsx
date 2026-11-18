import React, { useState } from 'react';
import { Layers, Cpu, Shield, Network, CheckCircle2 } from 'lucide-react';

type SectionTab = 'workspace' | 'drivers' | 'transport' | 'security' | 'roadmap';

export const RustArchitectureView: React.FC = () => {
  const [activeSection, setActiveSection] = useState<SectionTab>('workspace');
  const [uiFrameworkChoice, setUiFrameworkChoice] = useState<'tauri' | 'iced'>('tauri');
  const [linuxBackendChoice, setLinuxBackendChoice] = useState<'portal-uinput' | 'portal-only'>('portal-uinput');

  return (
    <div className="space-y-6">
      <div className="pb-5 border-b border-slate-200 dark:border-slate-800">
        <h1 className="text-xl font-semibold tracking-tight text-slate-900 dark:text-slate-100">
          SpanKVM — Rust 跨平台键鼠共享软件总体设计方案
        </h1>
        <p className="text-sm text-slate-600 dark:text-slate-400 mt-1">
          面向 macOS、Windows、Linux 的低延迟高刷新率 KVM 架构蓝图（待您审阅确认方案与界面样式后进入工程代码实现；右上角可随时导出独立单文件示例 HTML）。
        </p>
      </div>

      {/* Sub-navigation Tabs for Architecture Specification */}
      <div
        role="tablist"
        aria-label="Rust 架构方案章节切换"
        className="flex flex-wrap items-center gap-1.5 p-1.5 rounded-xl bg-slate-100 dark:bg-slate-900 border border-slate-200 dark:border-slate-800"
      >
        {(
          [
            { id: 'workspace', label: '01. 总体架构与 Crate 划分', icon: Layers },
            { id: 'drivers', label: '02. 三大系统底层驱动兼容', icon: Cpu },
            { id: 'transport', label: '03. 低延迟高刷与 QUIC 协议', icon: Network },
            { id: 'security', label: '04. 局域网配对与自动重连', icon: Shield },
            { id: 'roadmap', label: '05. 待确认技术选项与里程碑', icon: CheckCircle2 },
          ] as const
        ).map((tab) => {
          const Icon = tab.icon;
          const isActive = activeSection === tab.id;
          return (
            <button
              key={tab.id}
              role="tab"
              aria-selected={isActive}
              type="button"
              onClick={() => setActiveSection(tab.id)}
              className={`inline-flex items-center gap-1.5 px-3.5 py-2 text-xs rounded-lg transition-colors whitespace-nowrap shrink-0 ${
                isActive
                  ? 'bg-white dark:bg-slate-800 text-blue-700 dark:text-blue-300 font-semibold border border-slate-200/80 dark:border-slate-700 shadow-xs'
                  : 'font-medium text-slate-600 dark:text-slate-400 hover:text-slate-900 dark:hover:text-slate-200'
              }`}
            >
              <Icon className="w-3.5 h-3.5" />
              {tab.label}
            </button>
          );
        })}
      </div>

      {/* Section 01: Rust Workspace & Architecture Diagram */}
      {activeSection === 'workspace' && (
        <div className="space-y-6">
          <section className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl p-6 space-y-4">
            <h2 className="text-base font-semibold text-slate-900 dark:text-slate-100">
              01. 系统分层架构与数据通路图
            </h2>
            <p className="text-sm text-slate-600 dark:text-slate-400">
              软件采用「内核级低延迟守护协程（Rust Tokio + 独立实时输入线程） + 轻量级配置前端（Tauri v2 / WebView）」双层解耦架构：即使关闭配置窗口，后台驻留进程仍保持微秒级键鼠转发。
            </p>

            <pre className="p-4 rounded-lg bg-slate-950 text-slate-100 font-mono text-xs overflow-x-auto leading-relaxed">
{`┌──────────────────────────────────────────────────────────────────────────────┐
│                       SpanKVM 配置界面层 (Tauri v2 + HTML/React)              │
│   [最多 9 屏上下左右拓扑画布]   [临时密钥配对管理]   [按键映射表]   [拖拽策略]   │
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │ 本地 IPC (Typed Commands / Events)
┌───────────────────────────────────────▼──────────────────────────────────────┐
│                     spankvm-core (纯 Rust 核心状态机与路由器)                  │
│  • 9 节点正交连通图求解器 & 归一化屏幕边缘坐标映射 (DPI / 分辨率自适应比例缩放)   │
│  • 焦点跟随引擎：鼠标跨越屏幕边界瞬间切换 ActiveNodeId，键盘事件流实时重定向     │
│  • 扫描码级按键重映射表 (O(1) 数组查表转换，支持按目标 OS 动态改写修饰键状态)     │
│  • 按键防卡死看门狗 (跨屏或断线瞬间自动向旧屏幕补发 KeyUp，杜绝 Ctrl/Cmd 粘连)   │
└──────────────┬────────────────────────────────────────────────┬──────────────┘
               │ 专用高优先级实时线程 (OS Thread)                 │ Tokio 异步网络运行时
┌──────────────▼─────────────────────────┐      ┌───────────────▼──────────────┐
│     spankvm-pal (跨平台驱动抽象层)       │      │   spankvm-net (局域网通信层)   │
│ • macOS : CGEventTap / CGEventPost     │      │ • mdns-sd 纯局域网自动发现     │
│ • Win11 : Raw Input + LL Hook/SendInput│      │ • SPAKE2 6位临时密钥协商       │
│ • Linux : libei (Wayland) / uinput/XI2 │      │ • Quinn (QUIC + TLS 1.3 双向) │
│ • 剪贴板监听 & 边缘虚拟文件拖拽捕获器    │      │   ├─ 不可靠数据报: 1000Hz 鼠标 │
└────────────────────────────────────────┘      │   └─ 可靠多路流: 键盘/剪贴板/文件│
                                                └──────────────────────────────┘`}
            </pre>

            <div className="grid grid-cols-1 md:grid-cols-3 gap-4 pt-2">
              <div className="p-4 rounded-lg bg-slate-50 dark:bg-slate-950 border border-slate-200/80 dark:border-slate-800">
                <h3 className="text-sm font-semibold text-slate-900 dark:text-slate-100">
                  spankvm-core & proto
                </h3>
                <p className="text-xs text-slate-600 dark:text-slate-400 mt-1">
                  负责最多 9 台电脑的二维拓扑边缘判定、不同分辨率按物理高度百分比跨屏对齐、按键映射转换，以及零拷贝紧凑二进制帧序列化。
                </p>
              </div>
              <div className="p-4 rounded-lg bg-slate-50 dark:bg-slate-950 border border-slate-200/80 dark:border-slate-800">
                <h3 className="text-sm font-semibold text-slate-900 dark:text-slate-100">
                  spankvm-pal (驱动兼容层)
                </h3>
                <p className="text-xs text-slate-slate-600 dark:text-slate-400 mt-1 text-slate-600">
                  统一封装 InputCapture、InputInjector、ClipboardProvider 与 DragDropTarget 四大 Trait，针对三大桌面系统分别调用原生系统 API。
                </p>
              </div>
              <div className="p-4 rounded-lg bg-slate-50 dark:bg-slate-950 border border-slate-200/80 dark:border-slate-800">
                <h3 className="text-sm font-semibold text-slate-900 dark:text-slate-100">
                  spankvm-net (加密网络层)
                </h3>
                <p className="text-xs text-slate-600 dark:text-slate-400 mt-1">
                  基于 quinn + rustls 构建星型直连拓扑：主屏幕直连其余最多 8 台从机，消除多跳转发延迟。
                </p>
              </div>
            </div>
          </section>
        </div>
      )}

      {/* Section 02: Cross-Platform OS Driver Compatibility */}
      {activeSection === 'drivers' && (
        <section className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl overflow-hidden">
          <div className="p-6 border-b border-slate-200 dark:border-slate-800">
            <h2 className="text-base font-semibold text-slate-900 dark:text-slate-100">
              02. macOS / Windows / Linux 跨平台底层驱动兼容方案
            </h2>
            <p className="text-sm text-slate-600 dark:text-slate-400 mt-1">
              不依赖难签名的第三方内核态驱动（无需关闭 macOS SIP 或 Windows 驱动强制签名），全部采用官方合规低延迟用户态 HID / 系统事件接口。
            </p>
          </div>

          <div className="overflow-x-auto">
            <table className="w-full text-left border-collapse text-xs">
              <thead>
                <tr className="border-b border-slate-200 dark:border-slate-800 text-slate-500 dark:text-slate-400">
                  <th className="py-3.5 px-6 font-medium min-w-[168px] whitespace-nowrap">能力维度</th>
                  <th className="py-3.5 px-4 font-medium min-w-[230px]">macOS (Apple Silicon / Intel)</th>
                  <th className="py-3.5 px-4 font-medium min-w-[230px]">Windows 10 / 11</th>
                  <th className="py-3.5 px-6 font-medium min-w-[240px]">Linux (Wayland & X11 双栈)</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-200 dark:divide-slate-800 text-slate-700 dark:text-slate-300">
                <tr>
                  <td className="py-4 px-6 font-semibold text-slate-900 dark:text-slate-100 whitespace-nowrap align-top">
                    主屏键鼠捕获与冻结
                  </td>
                  <td className="py-4 px-4">
                    <span className="font-mono">CGEventTapCreate</span> (HID 层级拦截)；当焦点在远端屏幕时返回 <span className="font-mono">NULL</span> 吞掉本机事件并将光标锚定在边缘像素。
                  </td>
                  <td className="py-4 px-4">
                    <span className="font-mono">RegisterRawInputDevices</span> (<span className="font-mono">WM_INPUT</span>) 读取无加速原始 1000Hz 鼠标增量 + <span className="font-mono">WH_MOUSE_LL / WH_KEYBOARD_LL</span> 拦截本机响应。
                  </td>
                  <td className="py-4 px-6">
                    Wayland 优先使用 <span className="font-mono">InputCapture Portal</span> (<span className="font-mono">ashpd</span> + <span className="font-mono">libei</span>)；无 Portal 环境回退 <span className="font-mono">evdev EVIOCGRAB</span>；X11 使用 <span className="font-mono">XI2 RawMotion</span>。
                  </td>
                </tr>
                <tr>
                  <td className="py-4 px-6 font-semibold text-slate-900 dark:text-slate-100 whitespace-nowrap align-top">
                    从屏低延迟输入注入
                  </td>
                  <td className="py-4 px-4 align-top">
                    <span className="font-mono">CGEventPost</span> (<span className="font-mono">kCGHIDEventTap</span>) 配合连续像素级滚轮事件 (<span className="font-mono">kCGScrollWheelEventIsContinuous</span>)，完美保留触控板/高刷丝滑感。
                  </td>
                  <td className="py-4 px-4 align-top">
                    批量调用 <span className="font-mono">SendInput</span> (<span className="font-mono">KEYEVENTF_SCANCODE</span> 硬件扫描码级注入，兼容全屏游戏与远程软件) + 高精度滚轮 <span className="font-mono">WHEEL_DELTA</span> 细分。
                  </td>
                  <td className="py-4 px-6 align-top">
                    Wayland 使用 <span className="font-mono">RemoteDesktop Portal / libei</span> 注入，或通过 <span className="font-mono">/dev/uinput</span> 创建虚拟硬件键鼠设备；X11 使用 <span className="font-mono">XTestFakeMotionEvent</span>。
                  </td>
                </tr>
                <tr>
                  <td className="py-4 px-6 font-semibold text-slate-900 dark:text-slate-100 whitespace-nowrap align-top">
                    剪贴板与跨屏拖拽
                  </td>
                  <td className="py-4 px-4 align-top">
                    轮询 <span className="font-mono">NSPasteboard.changeCount</span> 延迟按需拉取；拖拽边缘唤起透明 <span className="font-mono">NSWindow</span> 承接 <span className="font-mono">NSFilenamesPboardType</span>。
                  </td>
                  <td className="py-4 px-4 align-top">
                    <span className="font-mono">AddClipboardFormatListener</span> 事件驱动监听；边缘触发 1px 透明 <span className="font-mono">IDropTarget</span> 捕获拖出文件并在目标端生成 <span className="font-mono">CF_HDROP</span>。
                  </td>
                  <td className="py-4 px-6 align-top">
                    Wayland <span className="font-mono">zwlr_data_control_v1 / wl-clipboard</span> 与 X11 <span className="font-mono">XFixesSelectSelectionInput</span>；拖拽文件落地至用户指定目录并注入 <span className="font-mono">text/uri-list</span>。
                  </td>
                </tr>
                <tr>
                  <td className="py-4 px-6 font-semibold text-slate-900 dark:text-slate-100 whitespace-nowrap align-top">
                    系统权限引导体验
                  </td>
                  <td className="py-4 px-4 align-top">
                    首次启动自动检测并弹窗引导授权「辅助功能 (Accessibility)」与「输入监控」。
                  </td>
                  <td className="py-4 px-4 align-top">
                    支持可选 UIAccess / 服务态提权，确保点击任务管理器或 UAC 提权窗口时不丢失键鼠控制。
                  </td>
                  <td className="py-4 px-6 align-top">
                    Wayland 下通过标准系统桌面授权弹窗确认；若使用 <span className="font-mono">uinput</span> 模式提供一键 <span className="font-mono">udev</span> 规则配置脚本（免 root 运行）。
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </section>
      )}

      {/* Section 03: Low Latency & High Refresh Rate Protocol */}
      {activeSection === 'transport' && (
        <section className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl p-6 space-y-5">
          <div>
            <h2 className="text-base font-semibold text-slate-900 dark:text-slate-100">
              03. 低延迟与高刷新率显示器（120Hz–360Hz）适配设计
            </h2>
            <p className="text-sm text-slate-600 dark:text-slate-400 mt-1">
              传统键鼠共享软件（如早期 Synergy/Barrier）基于单连接 TCP，一旦局域网出现单个 Wi-Fi 抖动丢包或同时传输大剪贴板图片，就会触发 TCP 队头阻塞（Head-of-Line Blocking），导致 144Hz/240Hz 屏幕上出现肉眼可见的光标卡顿与跳帧。
            </p>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-5">
            <div className="p-5 rounded-lg bg-slate-50 dark:bg-slate-950 border border-slate-200/80 dark:border-slate-800 space-y-2">
              <h3 className="text-sm font-semibold text-slate-900 dark:text-slate-100">
                双通道 QUIC 多路复用架构 (Rust `quinn`)
              </h3>
              <ul className="space-y-2 text-xs text-slate-600 dark:text-slate-400 leading-relaxed">
                <li>
                  • <strong className="text-slate-900 dark:text-slate-200">通道 A — QUIC 不可靠加密数据报 (RFC 9221)：</strong> 专用于承载 500Hz–1000Hz 鼠标相对位移 (<span className="font-mono">dx, dy</span>) 与高精度滚轮。单包仅 <span className="font-mono">14 Bytes</span>，无重传等待、零队头阻塞，千兆有线局域网端到端延迟稳定在 <span className="font-mono">0.3ms – 0.8ms</span>。
                </li>
                <li>
                  • <strong className="text-slate-900 dark:text-slate-200">通道 B — QUIC 独立优先级可靠流：</strong> 键盘按下/抬起使用最高优先级流（确保绝不漏键），剪贴板与文件拖拽走后台低优先级限速流，即使正在拖拽传输 500MB 文件，鼠标依然保持 240Hz/360Hz 满帧丝滑。
                </li>
              </ul>
            </div>

            <div className="p-5 rounded-lg bg-slate-50 dark:bg-slate-950 border border-slate-200/80 dark:border-slate-800 space-y-2">
              <h3 className="text-sm font-semibold text-slate-900 dark:text-slate-100">
                高刷屏幕抗抖动微插值与自适应合批
              </h3>
              <ul className="space-y-2 text-xs text-slate-600 dark:text-slate-400 leading-relaxed">
                <li>
                  • <strong className="text-slate-900 dark:text-slate-200">微秒级时间戳对齐：</strong> 每个鼠标数据报携带单调递增序列号与微秒时间戳；接收端结合目标显示器刷新率（如 144Hz / 240Hz / 360Hz）进行亚像素残差累加，消除整数取整引起的斜线锯齿感。
                </li>
                <li>
                  • <strong className="text-slate-900 dark:text-slate-200">零动态内存分配热路径：</strong> 输入捕获 → 键位重映射 → 加密发包全程在栈上固定尺寸 RingBuffer 完成，杜绝 GC 或堆分配停顿。
                </li>
              </ul>
            </div>
          </div>

          <pre className="p-4 rounded-lg bg-slate-950 text-slate-100 font-mono text-xs overflow-x-auto leading-relaxed">
{`// 紧凑二进制输入帧结构示例 (14 Bytes, Little-Endian, 零拷贝编解码)
#[repr(C, packed)]
pub struct FastPointerDatagram {
    pub frame_type: u8,       // 0x01 = MouseDelta, 0x02 = WheelHiRes
    pub target_node: u8,      // 0..=8 (支持最多 9 台电脑编址)
    pub seq: u32,             // 单调递增防乱序包序号
    pub timestamp_us: u32,    // 发送端单调时钟低 32 位微秒戳
    pub dx_subpx: i16,        // 相对位移 X (含高 DPI 比例换算)
    pub dy_subpx: i16,        // 相对位移 Y
}`}
          </pre>
        </section>
      )}

      {/* Section 04: LAN Temporary Key Pairing & Auto-Reconnect */}
      {activeSection === 'security' && (
        <section className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl p-6 space-y-5">
          <div>
            <h2 className="text-base font-semibold text-slate-900 dark:text-slate-100">
              04. 纯局域网隔离、临时密钥配对与自动重连安全协议
            </h2>
            <p className="text-sm text-slate-600 dark:text-slate-400 mt-1">
              键鼠事件等同于物理电脑的最高控制权，因此安全设计遵循「物理局域网边界锁定 + 口令增强密钥协商 (PAKE) + 双向证书钉扎」三重防护。
            </p>
          </div>

          <pre className="p-4 rounded-lg bg-slate-950 text-slate-100 font-mono text-xs overflow-x-auto leading-relaxed">
{`[主控电脑 A]                                                    [局域网待接入电脑 B]
     │  1. mDNS 广播 _spankvm._udp.local (校验源 IP 属于本机直连子网)        │
     │◄──────────────────────────────────────────────────────────────────────┤
     │                                               生成 6 位一次性临时码: 628-409 (TTL 300s)
     │  2. 用户输入 628-409 发起 SPAKE2 口令认证密钥交换 (防离线暴力枚举)     │
     ├──────────────────────────────────────────────────────────────────────►│
     │  3. 在加密隧道内互换长期 Ed25519 设备证书公钥指纹                     │
     │◄─────────────────────────────────────────────────────────────────────►│
     │  4. 若勾选「记录并下次自动配对」：存入系统原生 Keychain / DPAPI       │
     │                                                                       │
     │====== 下次开机 / 休眠唤醒同一局域网自动重连 (无需再次输入临时码) ======│
     │  5. mDNS 发现已知指纹 -> 直接发起双向 mTLS 1.3 (QUIC) 静默极速重连    │
     ├──────────────────────────────────────────────────────────────────────►│`}
          </pre>

          <div className="grid grid-cols-1 md:grid-cols-3 gap-4 text-xs text-slate-600 dark:text-slate-400">
            <div className="p-4 rounded-lg bg-slate-50 dark:bg-slate-950 border border-slate-200/80 dark:border-slate-800">
              <div className="font-semibold text-slate-900 dark:text-slate-100 mb-1">
                严格局域网围栏
              </div>
              监听套接字枚举本机私有网卡接口，主动丢弃任何非同一局域网网段或公网路由来源的数据包，零云端服务器依赖。
            </div>
            <div className="p-4 rounded-lg bg-slate-50 dark:bg-slate-950 border border-slate-200/80 dark:border-slate-800">
              <div className="font-semibold text-slate-900 dark:text-slate-100 mb-1">
                防暴力破解保护
              </div>
              6 位临时密钥基于 SPAKE2 协议（网络上不传输明文或哈希），连续输错 3 次立即销毁当前临时码并强制重新生成。
            </div>
            <div className="p-4 rounded-lg bg-slate-50 dark:bg-slate-950 border border-slate-200/80 dark:border-slate-800">
              <div className="font-semibold text-slate-900 dark:text-slate-100 mb-1">
                硬件级加速加密
              </div>
              基于 `ring` / `aws-lc-rs`，在支持 AES-NI 的 x86 与 ARMv8 芯片上自动协商 `AES-256-GCM` 或 `ChaCha20-Poly1305`，单包加解密耗时低于 2 微秒。
            </div>
          </div>
        </section>
      )}

      {/* Section 05: Technical Confirmation Gate & Milestones */}
      {activeSection === 'roadmap' && (
        <section className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl p-6 space-y-6">
          <div>
            <h2 className="text-base font-semibold text-slate-900 dark:text-slate-100">
              05. 开发前待您确认的工程选项与分阶段实施计划
            </h2>
            <p className="text-sm text-slate-600 dark:text-slate-400 mt-1">
              您可以在下方预选偏好架构方案，并在对话中告知确认或修改意见，确认后我们将正式生成完整 Rust Cargo Workspace 代码与前端绑定实现。
            </p>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-5">
            <div className="p-4 rounded-lg border border-slate-200 dark:border-slate-800 space-y-2.5">
              <div className="text-xs font-semibold text-slate-900 dark:text-slate-100">
                确认项 A：桌面端配置界面框架选型
              </div>
              <div className="space-y-2">
                <label className="flex items-start gap-2.5 text-xs cursor-pointer">
                  <input
                    type="radio"
                    name="ui-framework"
                    checked={uiFrameworkChoice === 'tauri'}
                    onChange={() => setUiFrameworkChoice('tauri')}
                    className="mt-0.5 text-blue-600 focus:ring-blue-600"
                  />
                  <span>
                    <strong className="text-slate-900 dark:text-slate-100">推荐：Tauri v2 (Rust 守护核心 + 当前 HTML/React 界面)</strong>
                    <span className="block text-slate-500 dark:text-slate-400 mt-0.5">
                      安装包仅 ~6MB，完全复用当前所见即所得的交互界面，关闭窗口后 WebView 自动释放内存，后台仅保留 ~12MB 纯 Rust 转发引擎。
                    </span>
                  </span>
                </label>
                <label className="flex items-start gap-2.5 text-xs cursor-pointer">
                  <input
                    type="radio"
                    name="ui-framework"
                    checked={uiFrameworkChoice === 'iced'}
                    onChange={() => setUiFrameworkChoice('iced')}
                    className="mt-0.5 text-blue-600 focus:ring-blue-600"
                  />
                  <span>
                    <strong className="text-slate-900 dark:text-slate-100">备选：纯 Rust 原生 GUI (`iced` / `egui`)</strong>
                    <span className="block text-slate-500 dark:text-slate-400 mt-0.5">
                      单二进制零 WebView 依赖，但复杂拖拽排版与中文字体渲染细腻度略逊于 Tauri v2。
                    </span>
                  </span>
                </label>
              </div>
            </div>

            <div className="p-4 rounded-lg border border-slate-200 dark:border-slate-800 space-y-2.5">
              <div className="text-xs font-semibold text-slate-900 dark:text-slate-100">
                确认项 B：Linux 桌面环境兼容范围
              </div>
              <div className="space-y-2">
                <label className="flex items-start gap-2.5 text-xs cursor-pointer">
                  <input
                    type="radio"
                    name="linux-backend"
                    checked={linuxBackendChoice === 'portal-uinput'}
                    onChange={() => setLinuxBackendChoice('portal-uinput')}
                    className="mt-0.5 text-blue-600 focus:ring-blue-600"
                  />
                  <span>
                    <strong className="text-slate-900 dark:text-slate-100">推荐：Wayland Portal (`libei`) + `/dev/uinput` + X11 全兼容双栈</strong>
                    <span className="block text-slate-500 dark:text-slate-400 mt-0.5">
                      同时覆盖 GNOME 46+/KDE Plasma 6/Hyprland 以及传统 Ubuntu X11 / 轻量窗口管理器。
                    </span>
                  </span>
                </label>
                <label className="flex items-start gap-2.5 text-xs cursor-pointer">
                  <input
                    type="radio"
                    name="linux-backend"
                    checked={linuxBackendChoice === 'portal-only'}
                    onChange={() => setLinuxBackendChoice('portal-only')}
                    className="mt-0.5 text-blue-600 focus:ring-blue-600"
                  />
                  <span>
                    <strong className="text-slate-900 dark:text-slate-100">精简：仅现代桌面标准 (`xdg-desktop-portal` + X11 `XI2`)</strong>
                    <span className="block text-slate-500 dark:text-slate-400 mt-0.5">
                      无需配置 `input` 用户组权限，适合主流发行版标准桌面。
                    </span>
                  </span>
                </label>
              </div>
            </div>
          </div>

          <div className="border-t border-slate-200 dark:border-slate-800 pt-4 space-y-2 text-xs text-slate-600 dark:text-slate-400">
            <div className="font-semibold text-slate-900 dark:text-slate-100">
              确认后工程实施分步路线：
            </div>
            <p>1. 搭建 Rust Cargo Workspace（`spankvm-proto`、`spankvm-core`、`spankvm-net`、`spankvm-pal`、`src-tauri`）与跨平台编译配置。</p>
            <p>2. 实现 mDNS 局域网发现、SPAKE2 6 位临时密钥配对状态机、Ed25519 证书持久化与 QUIC 双通道低延迟传输层。</p>
            <p>3. 实现 macOS / Windows / Linux 键鼠捕获注入、跨屏坐标与按键重映射引擎、剪贴板监听及文件分块拖拽传输，并与前端配置台完整联调。</p>
          </div>
        </section>
      )}
    </div>
  );
};
