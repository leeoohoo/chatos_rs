#!/bin/bash

# Double-clickable helper for the physical drive containing both
# MacPortable and WIN_UBUNTU. Install this file on the Mac's internal disk.

cd / || exit 1

EJECT_TOOL="$HOME/.local/bin/eject-drive"

if [ ! -x "$EJECT_TOOL" ]; then
  echo "没有找到推出工具：$EJECT_TOOL"
  echo "请先运行项目 scripts 目录中的 install-eject-macportable.sh。"
  status=1
elif [ -d /Volumes/WIN_UBUNTU ]; then
  echo "准备同时推出 MacPortable 和 WIN_UBUNTU……"
  "$EJECT_TOOL" "WIN_UBUNTU"
  status=$?
elif [ -d /Volumes/MacPortable ]; then
  echo "准备同时推出 MacPortable 和 WIN_UBUNTU……"
  "$EJECT_TOOL" "MacPortable"
  status=$?
else
  echo "没有发现 MacPortable 或 WIN_UBUNTU，硬盘可能已经推出。"
  status=1
fi

echo
if [ "$status" -eq 0 ]; then
  echo "两个卷都已安全推出，可以拔掉硬盘。"
else
  echo "未能推出硬盘，请查看上面的提示。"
fi
printf "按回车键关闭窗口……"
read -r _
exit "$status"
