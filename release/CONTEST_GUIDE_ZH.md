# Daedalus 1.3.1-contest（Linux x86_64）

这是实验室内部算法比赛版本，只支持 Linux x86_64。它从 Linux 1.3.1 发行版继承
渲染、物理、相机和 ABI，但运行时只开放两个地图：**靶场**与**能量机关**。能量机关地图
支持小符与大符；普通场和前哨场会被发行二进制拒绝。

## 一分钟开始

解压 `linux-x86_64.tar.gz` 后运行：

```bash
./install-linux.sh
daedalus-contest start --scene shooting-range
daedalus-contest status
```

使用同一个用户会话切换到能量机关：

```bash
daedalus-contest scene energy
daedalus-contest frame
daedalus-contest aim 0 90 --fire
daedalus-contest score red
daedalus-contest stop
```

比赛版本默认以可视渲染模式启动，参赛者直接执行 `daedalus-contest start` 即可看到
窗口。无桌面环境或性能压测时，显式使用 `daedalus-contest start --performance` 启动
无窗口高性能模式。若要并行做本机实验，为每个实例指定不同的运行目录和端口隔离环境；
正式比赛同一用户只启动一个实例。

## 本地车辆控制

窗口获得焦点后，竞赛版本默认支持参赛者直接控制自己的车辆：

- `W/A/S/D`：底盘移动；按住左 `Shift` 加速。
- 靶场中 `Q/E`：底盘左/右旋转。
- 能量机关中 `Q`：切换到小符；`E`：切换到大符。两种模式都按规则旋转，方向在本次运行内保持不变。
- 方向键或按住鼠标右键移动鼠标：控制云台。
- `Space`：按射击冷却连续发射。

竞赛版启动时默认由参赛者的键盘和鼠标控制本车；SDK 的图像、云台命令、发射和场景
控制接口可在同一运行中直接使用，不依赖窗口快捷键或内置自瞄开关。窗口内不显示旧
自瞄或 bridge 状态，也不显示未向比赛开放的截图、相机、面板或地图功能键提示。

进入大能量机关时，模拟器启动可推进的大符周期：初始同时点亮两个扇叶；命中其中一个后
保留另一目标进入短暂的次级窗口，随后自动生成下一对目标。超时、完成和失败都会按状态机
恢复并开始下一轮，不会固定在一次 SDK 场景切换时的单目标快照。

## C++ SDK

竞赛 SDK 只支持 C++17。`ContestClient` 将 TCP 图像、曝光同步云台姿态、UDP 控制和
场景控制合成一个接口；不提供目标真值、检测、PnP、预测器或模型管理。

```cpp
#include <daedalus_sim_sdk/contest_client.hpp>

using namespace daedalus::sim::sdk::v1;

ContestClientOptions options;
options.ipc_directory = "/tmp/daedalus-contest-1000";
ContestClient simulator(options);
if (!simulator.connect()) return 1;
simulator.selectScene(ContestScene::ShootingRange);

auto frame = simulator.nextFrame();  // 图像与同一 source_sequence 的实际云台状态
UdpGimbalCommand aim;
aim.yaw_deg = 0.0F;
aim.pitch_deg = 90.0F;
simulator.sendAim(aim);
```

制作可控标注数据时，使用受限的能量机关场景接口，而不是修改渲染资产。`RuleDriven`
严格使用规则转速模型；`Static` 停止转动，并且只接受五个扇叶的四种既有视觉状态。

```cpp
simulator.selectScene(ContestScene::Energy);
RuneScenario rule;
rule.mode = RuneMode::Large;
rule.motion = RuneMotion::RuleDriven;
simulator.setRuneScenario(rule);

RuneScenario frozen;
frozen.mode = RuneMode::Large;
frozen.motion = RuneMotion::Static;
frozen.leaf_states = {
    RuneLeafState::Activating, RuneLeafState::Activating,
    RuneLeafState::Deactivated, RuneLeafState::Deactivated,
    RuneLeafState::Deactivated};
simulator.setRuneScenario(frozen);
```

大能量机关的有效击打环数可直接读取。模拟器以物理接触点计算靶面径向位置：有效
检测直径为 300 mm，中心为 `10` 环，外缘为 `1` 环；`average_ring` 只统计本次大符
激活期间击中的有效亮扇叶（规则周期完成时为 5–10 个）。

```cpp
auto red_score = simulator.getBigRuneScore(RuneTeam::Red);
if (red_score && red_score.value->has_hit) {
    float average_ring = red_score.value->average_ring;
    std::uint8_t last_ring = red_score.value->last_ring;
}
```

命令行验收工具可用 `daedalus-contest score red` 或
`daedalus-contest score blue` 显示同一数据。该接口为只读；要控制大/小符、停止或
按规则转动，仍使用上面的 `RuneScenario`。

窗口内也会显示这些数据：在能量机关选择大符时，底部既有命中统计的 `pct` 右侧追加
`big-rune R arms=… avg=… last=… B arms=… avg=… last=…`。其中 `arms` 是本轮有效亮
扇叶命中数，`avg` 为本轮平均环数，`last` 为最近命中的 1–10 环。大符周期超时或完成后
数值会按规则重置；切换小符或离开能量机关时该行自动隐藏。

用 CMake 接入安装包中的 SDK：

```cmake
find_package(DaedalusSimSdk 1.3.1 REQUIRED CONFIG)
target_link_libraries(my_algorithm PRIVATE DaedalusSimSdk::DaedalusSimSdk)
```

`daedalus-contest-client` 是使用同一 SDK 的命令行验收工具。完整底层接口及固定 ABI
参见 `sdk/README.md` 和 `docs/sdk-contract.json`。
