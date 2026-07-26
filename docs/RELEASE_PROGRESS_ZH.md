# Daedalus 模拟器发布进度

更新日期：2026-07-26

## 当前版本

- 分支：`release/simulator-multiplatform-x86`
- 模拟器候选版本：`1.1.0`
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
- SDK/Release/标定/平台矩阵兼容检查通过。
- PowerShell、Bash、JSON 和 Git diff 格式检查通过。

### Windows 发行包验收（2026-07-26）

- 正式目录：`D:\仿真\releases\daedalus-simulator\1.1.0\windows-x86_64`。
- ZIP：`D:\仿真\releases\daedalus-simulator\1.1.0\windows-x86_64.zip`。
- 从发行包目录启动后保持运行；TCP 5602、UDP 5601/5603 全部就绪。
- Talos 元数据创建成功且大小为 76992 字节，无场景资源缺失。
- wgpu 自动选择 `NVIDIA GeForce RTX 4060 Laptop GPU`、`Dx12`、独立 GPU，
  `distribution_locked=true`。
- 使用发行包内 SDK 和 `find_package(DaedalusSimSdk 1.1)` 编译独立 C++ 验收消费者成功。
- 实际取得 1440×1080 RGBA32 图像（6220800 字节）、帧号和曝光时间戳，并按帧号取得同步
  云台状态。
- 场景控制 `ping/create_session/status` 均取得 ACK；云台命令的发送 ID 与
  `last_applied_command_id` 一致。
- 包内清单、SHA256、许可证、内部使用说明和两份中文文档完整；未发现模拟器实现源码、
  PDB、Cargo 工程、CUDA/TensorRT/ONNX 或模型文件。
- 原生验收发现并修复了 Visual Studio 多配置误用 Debug、并行 PDB 冲突、CMake 安装前缀
  转义以及无窗口高性能模式提前退出问题。

## 正式发布前剩余门禁

1. 在 Windows 上完成可视 Vulkan 模式人工画面验收。
2. 从最终提交生成 Linux 原生包，并在 Linux Vulkan GPU 上完成运行和 SDK 联调。
3. 将最终标定的物理曝光、增益和外参写入 `camera-calibration.json` 并提升标定 revision。
4. 当前包仅用于所属实验室内部非商业培训和研究；若要提供给其他组织或公开发布，
   必须另行审查许可证和第三方依赖。

## 发布规则

正式 ZIP 只能从干净且已提交的 revision 生成。包内清单记录源码 commit，并为每个文件
写入 SHA256。Windows 与 Linux 包不能混用二进制或 SDK 安装目录。
