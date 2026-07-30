# Daedalus 模拟器 1.1.1（Windows x86_64）

这是面向实验室成员的已编译模拟器，不包含模拟器源码，也不包含自瞄模型、CUDA、
TensorRT 或 ONNX。只运行模拟器不需要 Rust、CMake、Visual Studio 或 WSL。

## 一分钟安装

1. 将 ZIP **完整解压**到普通文件夹，不要直接在压缩软件中运行。
2. 双击根目录的 `setup.cmd`。
3. 默认安装到：

   ```text
   %LOCALAPPDATA%\DaedalusSimulator\1.1.1
   ```

4. 安装器会创建开始菜单项目：
   - `Daedalus Simulator`：高性能无窗口模式；
   - `Daedalus Simulator (Visible)`：Vulkan 可视验收模式。

安装不需要管理员权限，不修改系统 `PATH`，也不会安装显卡驱动。重新安装同一版本时，
`setup.cmd` 会先询问是否覆盖。命令行静默指定目录可使用：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\install-windows.ps1 `
  -InstallDir D:\Apps\DaedalusSimulator\1.1.1 -Force
```

## 运行所需依赖

| 依赖 | 必需性 | 说明 |
| --- | --- | --- |
| Windows 10/11 x64 | 必需 | 不支持 32 位 Windows、ARM Windows |
| 当前显卡驱动 | 必需 | 高性能模式需要 DX12；可视模式需要 Vulkan |
| Microsoft Visual C++ v14 x64 Runtime | 必需 | 缺少 `VCRUNTIME140.dll` 时安装官方 x64 运行库 |
| WSL | 不需要 | Windows 用户可完全在 Windows 中运行 |
| CUDA/TensorRT/模型 | 模拟器不需要 | 仅用户自己的自瞄程序可能需要 |

Microsoft 官方 VC++ x64 Runtime：
<https://aka.ms/vc14/vc_redist.x64.exe>

安装脚本会检查 `VCRUNTIME140.dll`。显卡驱动请从电脑或显卡厂商的官方渠道安装；本包
不会静默下载或修改驱动。

## 不安装，直接运行

本包也是便携版。完整解压后可在 PowerShell 中运行：

```powershell
# 高性能模式：没有主窗口是正常现象，图像仍通过 SDK/TCP 输出
.\start-simulator.ps1

# 可视模式：用于检查场景和画面
.\start-simulator.ps1 -Visible
```

默认端口：云台 `5601/UDP`、图像 `5602/TCP`、场景控制 `5603/UDP`。同一台电脑不要同时
启动两个模拟器实例。

## 使用 C++ SDK 时才需要的工具

只运行模拟器不用安装以下工具。开发自瞄消费者时建议使用：

- Visual Studio 2022 Build Tools（MSVC x64）；
- CMake 3.16 或更高版本；
- 支持 C++17 的编译器。

```cmake
find_package(DaedalusSimSdk 1.1 REQUIRED CONFIG)
target_link_libraries(my_autoaim PRIVATE DaedalusSimSdk::DaedalusSimSdk)
```

```powershell
cmake -S . -B build `
  -DCMAKE_PREFIX_PATH="<安装目录>\sdk"
cmake --build build --config Release
```

## 包内目录

| 路径 | 用途 |
| --- | --- |
| `bin/` | 模拟器可执行文件 |
| `assets/` | 只读场景资源 |
| `sdk/include/` | C++ SDK 公共头文件 |
| `sdk/lib/` | Windows x64 SDK 库及 CMake package |
| `docs/SIMULATOR_USER_GUIDE_ZH.md` | 完整中文使用手册和性能简报 |
| `docs/SDK_API_REFERENCE_ZH.md` | 所有 SDK 函数、参数、返回值和示例 |
| `docs/SIMULATOR_TROUBLESHOOTING.md` | 常见故障排查 |
| `camera-calibration.json` | 固定只读相机内外参和曝光 |
| `release.json` | 版本、端口和 ABI 契约 |
| `release-manifest.json` | 包内文件大小与 SHA256 |

## 卸载

模拟器没有系统服务和驱动。关闭模拟器后，删除安装目录和开始菜单中的
`Daedalus Simulator` 文件夹即可。用户自己的自瞄工程、模型和运行日志不在安装目录时
不会受影响。

## 首次排查

- 高性能模式没有窗口：正常；用 SDK 连接 TCP 5602 判断图像流。
- 提示缺少 `VCRUNTIME140.dll`：安装上面的 Microsoft x64 Runtime。
- 可视模式无法启动：更新显卡驱动并确认 Vulkan 可用。
- 端口占用：确认已有模拟器是否仍在运行，不要直接结束来源不明的进程。
- 实际后端和显卡写入运行目录的 `daedalus-runtime-capabilities-v1.json`。

更详细说明请从 `docs/SIMULATOR_USER_GUIDE_ZH.md` 开始阅读。
