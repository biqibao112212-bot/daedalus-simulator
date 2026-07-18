# 模拟器模块上下文

- 上下文版本：`CTX-SIM-2026.07-v2`
- 分支：`integration/perf-main`
- 工作目录：`D:\仿真\worktrees\main-performance`
- 代码基线：本文件所在提交
- 模拟器版本：`Daedalus Simulator 1.0.0`
- 公共 SDK：`DaedalusSimSdk 1.0.0`
- IPC 布局：`SHM v7`
- 固定图像规格：RGB24，`1440×1080`

本分支是模拟器唯一权威实现，负责渲染、物理、场景、采集、真值、控制通道、性能基线与 SDK 发布。自瞄和打符只能依赖已安装的公共 SDK 与公开运行参数，不再复制或手写 IPC 结构，也不得要求在各自分支维护一份模拟器补丁。

当前完成的是解耦第一版：Rust 生产端保留在本分支；C++ 公共 ABI 由 `sdk/cpp` 单点发布；B 分支已作为首个消费者完成适配。其他消费者尚未适配，但以后必须使用同一 SDK。

公共入口：

- SDK 契约：`sdk/contract.json`
- C++ 头文件：`sdk/cpp/include/daedalus_sim_sdk/talos_v1.hpp`
- 接口说明：`agent-team/SIMULATOR_INTERFACE.md`
- 场景控制：`agent-team/SCENARIO_CONTROL.md`
- 构建与性能：`SIMULATOR_PERFORMANCE.md`
