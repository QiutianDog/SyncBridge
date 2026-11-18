#!/usr/bin/env bash
# SpanKVM Linux Wayland / uinput 免 root 权限配置脚本（确认项 B 双栈兼容）
# 作用：当桌面合成器暂未实现 org.freedesktop.portal.InputCapture (libei) 时，
#       允许当前登录用户通过 input 用户组读写 /dev/uinput 与 /dev/input/event*。

set -euo pipefail

RULE_FILE="/etc/udev/rules.d/99-spankvm-uinput.rules"
TARGET_USER="${SUDO_USER:-$USER}"

echo "[SpanKVM] 正在为用户 ${TARGET_USER} 配置 Linux Wayland / uinput 兼容权限..."

sudo groupadd -f input
sudo usermod -aG input "${TARGET_USER}"

sudo tee "${RULE_FILE}" >/dev/null <<'EOF'
KERNEL=="uinput", SUBSYSTEM=="misc", MODE="0660", GROUP="input", OPTIONS+="static_node=uinput", TAG+="uaccess"
KERNEL=="event*", SUBSYSTEM=="input", MODE="0660", GROUP="input", TAG+="uaccess"
EOF

sudo modprobe uinput
sudo udevadm control --reload-rules
sudo udevadm trigger

echo "[SpanKVM] 配置完成！重新登录桌面会话后即可免 root 启用 /dev/uinput 后备驱动。"
