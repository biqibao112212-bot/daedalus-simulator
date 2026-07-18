# 模拟器模块上下文

- 上下文版本：`CTX-SIM-REPO-2026.07-v2`
- 仓库：`daedalus-simulator`
- 分支：`main`
- 工作目录：`D:\仿真\repos\daedalus-simulator`
- 代码基线：本文件所在提交
- 模拟器版本：`Daedalus Simulator 1.0.1`
- 公共 SDK：`DaedalusSimSdk 1.0.0`
- IPC 布局：`SHM v7`
- 固定图像规格：RGB24，`1440×1080`

本仓库是模拟器唯一权威实现，负责渲染、物理、场景、采集、真值、控制通道、性能基线、SDK 与正式发布包。`D:\仿真\repos\aim-stack` 只能依赖发布包和公共 SDK，不得复制或手写 IPC 结构。

来自消费者调试的模拟器 bug 或需求必须先形成具体提案并由用户明确批准。批准前模拟器仓库保持只读；批准后修改仍由本仓库独立完成并通过新 Release/SDK 传播，消费者不能直接修改或携带模拟器实现。

当前完成独立仓库拆分：Rust 生产端、公共 C++ SDK、版本契约和 Release 工具均由本仓库单点发布。自瞄 B 是首个消费者；打符适配状态由消费者仓库维护。

公共入口：

- SDK 契约：`sdk/contract.json`
- C++ 头文件：`sdk/cpp/include/daedalus_sim_sdk/talos_v1.hpp`
- TCP 图像协议：`sdk/cpp/include/daedalus_sim_sdk/tcp_image_v1.hpp`
- 端点定义：`sdk/cpp/include/daedalus_sim_sdk/endpoints_v1.hpp`
- 接口说明：`agent-team/SIMULATOR_INTERFACE.md`
- 场景控制：`agent-team/SCENARIO_CONTROL.md`
- 构建与性能：`SIMULATOR_PERFORMANCE.md`
- 正式发布：`RELEASE.md`
