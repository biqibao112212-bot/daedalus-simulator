# 模拟器场景控制接口

- 上下文版本：`CTX-SIM-2026.07-v2`
- 状态：公开配置入口，SDK 控制 API 待下一版固化

当前消费者可使用以下稳定类别，具体可选值以模拟器源码校验为准：

- `DAEDALUS_SCENE_MODE` / `DAEDALUS_AUTO_AIM_MODE`：装甲板、前哨站或打符场景。
- `DAEDALUS_CAPTURE_SCENE_PROFILE`：完整场景或面向算法的精简采集层。
- `DAEDALUS_AUTO_GEN_SEED`：确定性生成种子。
- 自动生成参数：目标类型、距离、yaw/pitch 扫描、抖动、稳定帧、光照、打符模式/队伍/状态。
- 靶场参数：目标距离、编号、初始位姿和初始 yaw。
- 云台命令：统一通过 SDK 的 `GimbalCommand`；自瞄和打符不得建立私有命令结构。

实现入口位于 `src/auto_gen.rs`、`src/setup.rs`、`src/capture.rs` 和 `src/talos/plugin.rs`。在 SDK 控制 API 发布前，任何新增场景控制都先在本分支实现并写入本文件；消费者只能传公开参数，不能修改实体内部结构。

下一版控制 API 必须覆盖：创建确定性会话、reset/step/run、目标生成与销毁、身份/队伍/类型、位姿和运动轨迹、装甲板/打符状态、相机/云台初态、光照/采集配置、确认响应与错误码。
