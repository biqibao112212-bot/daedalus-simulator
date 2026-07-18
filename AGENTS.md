# 模拟器分支指令

本仓库固定为独立模拟器仓库 `D:\仿真\repos\daedalus-simulator`，正式分支为 `main`。开始前核对仓库和分支，只读取本仓库的 `agent-team/PROJECT.md`、`BOARD.md`、`DECISIONS.md`；公共契约另读 `SIMULATOR_INTERFACE.md`、`SCENARIO_CONTROL.md` 和仓库根 `SIMULATOR_PERFORMANCE.md`。

本仓库是渲染、物理、采集、场景、真值、控制、SDK 和 Release 的唯一权威实现。消费者算法不得进入本仓库；模拟器源码不得复制到消费者仓库。兼容性和性能结论必须来自带版本和清单的 Release。

审批门禁标识：`SIMULATOR_CHANGE_APPROVAL_REQUIRED`。如果问题是在自瞄、火控或打符调试中发现的，而用户尚未在收到具体复现、证据、影响、建议公共接口和版本变化之后明确批准修改模拟器，则本仓库只能进行只读诊断，禁止编辑源码、SDK、发布脚本或正式 Release。泛化的“继续调试自瞄”“修好问题”不自动构成模拟器写授权。获批后才可在本仓库独立修改、测试、发布新版本，再由消费者更新锁；不得在消费者分支实施模拟器改动。
