#!/bin/bash

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
BIN_DIR="$HOME/.local/bin"
DESKTOP_DIR="$HOME/Desktop"

mkdir -p "$BIN_DIR" "$DESKTOP_DIR"
install -m 755 "$SCRIPT_DIR/eject-drive.sh" "$BIN_DIR/eject-drive"
install -m 755 "$SCRIPT_DIR/eject-macportable.command" "$DESKTOP_DIR/推出 MacPortable 硬盘.command"

echo "安装完成：$DESKTOP_DIR/推出 MacPortable 硬盘.command"
echo "以后双击桌面上的这个文件即可。"
