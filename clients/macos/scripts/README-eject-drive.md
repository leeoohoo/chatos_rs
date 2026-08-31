# 安全推出移动硬盘（macOS）

脚本位置：`scripts/eject-drive.sh`

先赋予执行权限（仓库中的文件通常已经有权限）：

```bash
chmod +x scripts/eject-drive.sh
```

按 Finder 中显示的卷名推出：

```bash
./scripts/eject-drive.sh "My SSD"
```

也可以传入挂载路径或磁盘编号：

```bash
./scripts/eject-drive.sh "/Volumes/My SSD"
./scripts/eject-drive.sh disk4
```

脚本会先尝试正常卸载。如果失败，它会列出占用硬盘的用户程序，并在得到确认后请求这些程序退出，然后再次卸载并弹出硬盘。

如果想跳过确认：

```bash
./scripts/eject-drive.sh --yes "My SSD"
```

只有在明确接受未保存数据可能丢失时，才使用 `--force`：

```bash
./scripts/eject-drive.sh --yes --force "My SSD"
```

脚本不会主动结束 root/system 进程，也不会结束当前脚本的父终端。如果终端当前目录位于移动硬盘中，请先运行 `cd /`。

## 这台电脑上的 MacPortable / WIN_UBUNTU

这两个卷属于同一块物理硬盘。运行脚本并传入其中任一卷名，就会同时卸载并推出整块硬盘：

```bash
./scripts/eject-drive.sh "WIN_UBUNTU"
```

不要把唯一一份脚本放在 `MacPortable` 上直接运行；脚本文件本身可能成为占用者。建议把它安装到 Mac 内置盘，例如 `~/.local/bin/`。
