# Daedalus 模拟器发布进度

更新日期：2026-07-26

## 当前版本

- 分支：`release/simulator-multiplatform-x86`
- 模拟器候选版本：`1.1.1`
- SDK：`1.1.0`
- Talos SHM：v7，ABI revision 2
- 目标平台：Windows x86_64、Linux x86_64

开发工作在独立 worktree 完成。原 `main` 仓库在开发期间未切换分支、未写入，避免与
当时运行中的模拟器进程冲突。

## 已完成

- Windows/Linux 原生构建、SDK 安装和平台分包脚本。
- 编译期 `distribution-release` 锁定模式。
- 发布包排除 Rust/C++ 实现源码、调试符号、可编辑配置和推理模型。
- 固定只读相机标定契约；无运行时标定 setter。
- wgpu 高性能 GPU 自动选择及实际 adapter/backend/driver 查询。
- TCP latest-only 图像、帧号和曝光时间戳。
- 按图像帧号查询曝光同步云台角和底盘/云台/相机位姿，保留最近 16 帧。
- 云台绝对角、距离和开火建议发送；命令 ID 和实际状态回读。
- 装甲板、能量机关、前哨站和靶场场景控制。
- 发布版关闭本地键鼠修改、调试 UI、数据集生成、managed inference bridge 和目标真值。
- 中文初版使用手册和 C++ 闭环示例。
- 独立的 C++ SDK 函数参考，包含参数、返回值、错误处理和调用示例。
- 实验室内部非商业发行配置；打包不再依赖商业许可证文件。

## 已完成验证

- `cargo check --locked --features talos,distribution-release` 通过。
- Talos IPC 7 项 Rust 单元测试通过。
- Linux C++ SDK 7 项 CTest 通过。
- Windows 原生 `x86_64-pc-windows-msvc` Release + thin LTO 构建通过。
- Windows MSVC C++ SDK Release/x64 构建、安装和 7 项 CTest 全部通过。
- Linux 原生 `x86_64-unknown-linux-gnu` Release + thin LTO 构建通过；Linux GNU C++ SDK
  构建、安装和 7 项 CTest 全部通过。
- SDK/Release/标定/平台矩阵兼容检查通过。
- PowerShell、Bash、JSON 和 Git diff 格式检查通过。

### Windows 发行包验收（2026-07-26）

- 正式目录：`D:\仿真\releases\daedalus-simulator\1.1.1\windows-x86_64`。
- ZIP：`D:\仿真\releases\daedalus-simulator\1.1.1\windows-x86_64.zip`。
- 从发行包目录启动后保持运行；TCP 5602、UDP 5601/5603 全部就绪。
- Talos 元数据创建成功且大小为 76992 字节，无场景资源缺失。
- wgpu 自动选择 `NVIDIA GeForce RTX 4060 Laptop GPU`、`Dx12`、独立 GPU，
  `distribution_locked=true`。
- 可视模式以 Vulkan 启动，实际选择同一 RTX 4060 独显；窗口可响应，靶车、装甲板、场景
  和状态叠加层均正常显示，无黑屏、资源缺失或渲染破损。
- 使用发行包内 SDK 和 `find_package(DaedalusSimSdk 1.1)` 编译独立 C++ 验收消费者成功。
- 实际取得 1440×1080 RGBA32 图像（6220800 字节）、帧号和曝光时间戳，并按帧号取得同步
  云台状态。
- 场景控制 `ping/create_session/status` 均取得 ACK；云台命令的发送 ID 与
  `last_applied_command_id` 一致。
- 包内清单、SHA256、许可证、内部使用说明和两份中文文档完整；未发现模拟器实现源码、
  PDB、Cargo 工程、CUDA/TensorRT/ONNX 或模型文件。
- 原生验收发现并修复了 Visual Studio 多配置误用 Debug、并行 PDB 冲突、CMake 安装前缀
  转义以及无窗口高性能模式提前退出问题。

### Linux 发行包验收（2026-07-26）

- 正式目录：`D:\仿真\releases\daedalus-simulator\1.1.1\linux-x86_64`；推荐 TAR.GZ
  和备用 ZIP 位于同级目录。
- 二进制确认是 `ELF 64-bit LSB PIE x86-64`，动态依赖仅为常规 Linux 系统库，无 Windows
  DLL 或 WSL 专用依赖。
- 在 Ubuntu 22.04 WSL2 中从包目录外启动成功；TCP 5602、UDP 5601/5603 和 76992 字节
  Talos 元数据均就绪，`distribution_locked=true`，无资源缺失或 panic。
- 包内 SDK 能由独立 GNU C++ 消费者通过 `find_package(DaedalusSimSdk 1.1)` 编译链接。
- 本机 Linux Vulkan loader 仅暴露 Mesa llvmpipe CPU 适配器。按实验室内部发布标准，Linux
  只要求无独显兼容性验收：软件 Vulkan 启动、端口、IPC 和实际 adapter 能力查询均通过。
- llvmpipe 在 5 分钟内没有生成固定 1440×1080 首帧，因此 Linux 无独显模式只承诺构建、
  启动和接口兼容诊断，不承诺实时取图、自瞄帧率或完整活体图像闭环。需要 Linux 实时
  自瞄时仍应使用 Vulkan 能实际访问的硬件 GPU，但这不再是本版本发布门禁。

### 固定相机契约

- `camera-calibration.json` 已提升为 `daedalus-camera-1440x1080-v2` / revision 2。
- 内参、畸变和 `T_gimbal_camera` 保持固定只读。
- RGB 相机明确固定为数字曝光 `EV100=9.7`、关闭自动曝光、`Tonemapping=None`；物理快门
  时间和模拟增益不适用于数字渲染器，不再保留空值占位。

## 本版本验收边界

Windows 完成完整图像与控制闭环验收；Linux 完成无独显软件 Vulkan 的构建、启动、端口、
IPC、能力上报和 SDK 静态测试验收。当前包仅用于所属实验室内部非商业培训和研究；若要
提供给其他组织或公开发布，必须另行审查许可证和第三方依赖。

## 发布规则

正式 ZIP 只能从干净且已提交的 revision 生成。包内清单记录源码 commit，并为每个文件
写入 SHA256。Windows 与 Linux 包不能混用二进制或 SDK 安装目录。
