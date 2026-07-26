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

## 已完成验证

- `cargo check --locked --features talos,distribution-release` 通过。
- Talos IPC 7 项 Rust 单元测试通过。
- Linux C++ SDK 7 项 CTest 通过。
- SDK/Release/标定/平台矩阵兼容检查通过。
- PowerShell、Bash、JSON 和 Git diff 格式检查通过。

## 正式发布前剩余门禁

1. 在安装 CMake/MSVC 的 Windows 发布机完成原生 SDK CTest 和完整 Release 构建。
2. 在 Windows 与 Linux 实际 GPU 上分别运行可视和高性能模式冒烟测试。
3. 用示例消费者完成图像—同步云台状态—命令—实际状态闭环验收。
4. 将最终标定的物理曝光、增益和外参写入 `camera-calibration.json` 并提升标定 revision。
5. 当前仓库许可证是 AGPL-3.0；若坚持二进制闭源分发，发布前必须提供经确认的
   `release/COMMERCIAL_LICENSE.txt`。打包脚本会主动阻止缺少该文件的正式包。

## 发布规则

正式 ZIP 只能从干净且已提交的 revision 生成。包内清单记录源码 commit，并为每个文件
写入 SHA256。Windows 与 Linux 包不能混用二进制或 SDK 安装目录。
