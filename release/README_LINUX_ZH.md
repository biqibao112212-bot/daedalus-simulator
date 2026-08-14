# Daedalus 模拟器 1.3.1（Linux x86_64）

这是面向实验室成员的已编译模拟器，不包含模拟器源码，也不包含自瞄模型、CUDA、
TensorRT 或 ONNX。Linux 用户不需要 Windows 或 WSL。

## 一分钟安装

推荐下载 `linux-x86_64.tar.gz`，它会保留 Linux 可执行权限：

```bash
mkdir daedalus-simulator-1.3.1
tar -xzf linux-x86_64.tar.gz -C daedalus-simulator-1.3.1
cd daedalus-simulator-1.3.1
./install-linux.sh
```

如果下载的是备用 ZIP：

```bash
mkdir daedalus-simulator-1.3.1
unzip linux-x86_64.zip -d daedalus-simulator-1.3.1
cd daedalus-simulator-1.3.1
chmod +x install-linux.sh start-simulator.sh
./install-linux.sh
```

默认安装位置：

```text
~/.local/opt/daedalus-simulator/1.3.1
```

默认创建两个命令：

```text
~/.local/bin/daedalus-simulator
~/.local/bin/daedalus-simulator-visible
```

如果 `~/.local/bin` 不在 `PATH`，可以直接使用完整路径，或将下面一行加入 shell 配置：

```bash
export PATH="$HOME/.local/bin:$PATH"
```

自定义安装位置：

```bash
./install-linux.sh --prefix /path/to/daedalus/1.3.1
```

该安装方式不需要 root；只有安装缺失的系统依赖时可能需要 `sudo`。

## 运行所需依赖

| 依赖 | 必需性 | 说明 |
| --- | --- | --- |
| x86_64 Linux | 必需 | 不支持 i686 或 ARM |
| glibc、`libudev.so.1`、`libasound.so.2` | 必需 | 安装器通过 `ldd` 检查 |
| Vulkan loader | 必需 | 通常由 `libvulkan1` 提供 |
| Vulkan 驱动 | 必需 | 实时取图建议使用硬件 GPU；llvmpipe 仅用于启动诊断 |
| X11/Wayland | 仅可视模式需要 | 高性能无窗口模式可直接运行在无 `DISPLAY` 的计算节点 |
| Windows/WSL | 不需要 | Linux 包原生运行 |
| CUDA/TensorRT/模型 | 模拟器不需要 | 仅用户自己的自瞄程序可能需要 |

Ubuntu 22.04 / Debian 系常用安装命令：

```bash
sudo apt update
sudo apt install libvulkan1 mesa-vulkan-drivers libudev1 libasound2
```

其他发行版请安装能提供上述 `.so` 的等价软件包。可使用下面的命令检查：

```bash
ldd bin/daedalus | grep 'not found'
vulkaninfo --summary
```

### AutoDL / 无桌面 NVIDIA 服务器

若 `nvidia-smi` 正常、但 Vulkan 在无 `DISPLAY` 环境找不到显卡，AutoDL 镜像中的默认
NVIDIA ICD 可能仍指向依赖 X11 的 `libGLX_nvidia.so.0`。请按 AutoDL 官方 Vulkan 文档把
无显示 ICD 指向 `libEGL_nvidia.so.0`，不要为此替换模拟器或推理模型：

<https://api.autodl.com/docs/vulkan/>

从 GitHub 下载较慢时可使用 AutoDL 官方学术加速通道：

<https://www.autodl.com/docs/network_turbo/>

## 启动

安装后：

```bash
# 高性能无窗口模式
daedalus-simulator

# 可视模式
daedalus-simulator-visible
```

服务器没有桌面环境时使用高性能无窗口模式，不需要安装 Xvfb；可视模式需要有效的
`DISPLAY` 或 Wayland 会话。

便携运行：

```bash
./start-simulator.sh
./start-simulator.sh --visible
./start-simulator.sh --ipc-dir /tmp/daedalus/talos-ipc
```

默认端口：云台 `5601/UDP`、图像 `5602/TCP`、场景控制 `5603/UDP`。同一用户环境不要同时
启动两个模拟器实例。

## 使用 C++ SDK 时才需要的工具

只运行模拟器不需要编译器。开发自瞄消费者时需要支持 C++17 的 GNU 工具链和 CMake：

```bash
sudo apt install build-essential cmake
cmake -S . -B build \
  -DCMAKE_PREFIX_PATH="$HOME/.local/opt/daedalus-simulator/1.3.1/sdk"
cmake --build build --parallel
```

```cmake
find_package(DaedalusSimSdk 1.3.1 REQUIRED CONFIG)
target_link_libraries(my_autoaim PRIVATE DaedalusSimSdk::DaedalusSimSdk)
```

## 包内目录

| 路径 | 用途 |
| --- | --- |
| `bin/` | Linux x86_64 模拟器可执行文件 |
| `assets/` | 只读场景资源 |
| `sdk/include/` | C++ SDK 公共头文件 |
| `sdk/lib/` | Linux x86_64 SDK 库及 CMake package |
| `docs/SIMULATOR_USER_GUIDE_ZH.md` | 完整中文使用手册和性能简报 |
| `docs/SDK_API_REFERENCE_ZH.md` | 所有 SDK 函数、参数、返回值和示例 |
| `docs/OFFLINE_EXACT_CORNER_EXPORT_ZH.md` | 默认关闭的离线同曝光角点导出合同与实验步骤 |
| `schemas/offline-exact-corners-v1.schema.json` | exact-corner JSONL 机器可读 schema |
| `docs/SIMULATOR_TROUBLESHOOTING.md` | 常见故障排查 |
| `camera-calibration.json` | 固定只读相机内外参和曝光 |
| `release.json` | 版本、端口和 ABI 契约 |
| `release-manifest.json` | 包内文件大小与 SHA256 |

## 卸载

默认安装可执行：

```bash
rm -rf -- "$HOME/.local/opt/daedalus-simulator/1.3.1"
rm -f -- "$HOME/.local/bin/daedalus-simulator" \
  "$HOME/.local/bin/daedalus-simulator-visible"
```

## 首次排查

- `ldd` 显示 `not found`：安装对应系统库。
- Vulkan 初始化失败：检查 `vulkaninfo --summary` 和系统 Vulkan 驱动。
- llvmpipe 能启动但取图极慢：这是 CPU 软件 Vulkan 的限制；实时自瞄请使用硬件 GPU。
- 端口占用：确认已有模拟器是否仍在运行，不要结束来源不明的进程。
- 实际后端和适配器写入 IPC 目录的 `daedalus-runtime-capabilities-v1.json`。

更详细说明请从 `docs/SIMULATOR_USER_GUIDE_ZH.md` 开始阅读。

离线训练标签必须显式使用
`./start-simulator.sh --corner-labels-jsonl <安装目录之外的新绝对.jsonl路径>` 启用；它不会
解锁 SDK 在线目标真值。采集与 free-IPPE 验收命令见
`docs/OFFLINE_EXACT_CORNER_EXPORT_ZH.md`。
