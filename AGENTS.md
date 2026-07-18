# 模拟器分支指令

本工作树固定为 `integration/perf-main`。开始前核对分支，只读取本分支的 `agent-team/PROJECT.md`、`BOARD.md`、`DECISIONS.md`；公共契约另读 `SIMULATOR_INTERFACE.md`、`SCENARIO_CONTROL.md` 和仓库根 `SIMULATOR_PERFORMANCE.md`。

本分支是渲染、物理、采集、场景、真值、控制和 SDK 的唯一权威实现。消费者专用算法不得进入本分支；模拟器源码不得复制到功能分支。兼容性和性能结论必须来自 Release 与可复现测试。
