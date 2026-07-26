# 模拟器场景控制接口

- 上下文版本：`CTX-SIM-2026.07-v2`
- 状态：`DaedalusSimSdk 1.1.0` 场景控制 v1 已固化

启动环境变量仍保留用于人工调试和批量启动，但消费者运行时控制统一使用 SDK，不再直接修改模拟器内部资源。

## 运行时控制

`SceneControlClient` 使用 `127.0.0.1:5603/udp`（可由 `DAEDALUS_SCENE_CONTROL_BIND` 覆盖），协议名为 `daedalus.scene-control/1`。请求和响应都携带 `command_id`、`session_id`；响应包含状态、应用帧、时间戳和消息。

v1 支持：

- `ping / create_session / status`；
- `set_scene`：`armor / energy / outpost / shooting_range`；
- `reset_scene`：强制重建当前场景；
- `set_range_target_motion`：控制 1/3 号靶车的静止、直线、自转或直线+自转参数；
- `set_rune_state`：关闭、小符或大符，以及待激活/已激活叶片集合。

所有 World 修改均在 Bevy 主线程执行。场景切换和 reset 的 ACK 在实体重建完成后发送；非法参数、未创建会话、会话不一致和实体未就绪均返回明确错误，不做猜测。

## 启动配置入口

以下稳定类别仍可用于确定初始状态，具体可选值以本文件和 SDK 为准：

- `DAEDALUS_SCENE_MODE` / `DAEDALUS_AUTO_AIM_MODE`：装甲板、前哨站或打符场景。
- `DAEDALUS_CAPTURE_SCENE_PROFILE`：完整场景或面向算法的精简采集层。
- `DAEDALUS_AUTO_GEN_SEED`：确定性生成种子。
- 自动生成参数：目标类型、距离、yaw/pitch 扫描、抖动、稳定帧、光照、打符模式/队伍/状态。
- 靶场参数：目标距离、编号、初始位姿和初始 yaw。
- 云台命令：统一通过 SDK 的 `GimbalCommand`；自瞄和打符不得建立私有命令结构。

实现入口位于 `src/scene_control.rs`、`src/setup.rs`、`src/systems/range_control.rs` 和 `src/robomaster/power_rune/rune.rs`。任何新增跨模块控制必须先在模拟器仓库升级协议与 SDK，再由消费者升级版本锁；消费者不能修改实体内部结构。

v1 的正式边界是固定场景、固定靶车和打符状态控制。任意实体生成/销毁、暂停/单步、运行时相机和光照变更尚未进入 1.0 契约；需要时在后续 SDK 小版本中增加，不得由消费者建立私有旁路。
