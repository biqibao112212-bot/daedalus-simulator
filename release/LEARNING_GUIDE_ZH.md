# Daedalus 1.4.0-learning-r2（Linux x86_64）

这是从最新比赛版 `1.3.1-contest-r2` 派生的实验室学习版本。它保留靶场、能量机关、
键盘/鼠标车辆控制、SDK 云台/开火控制、装甲板命中判定和大小能量机关；但它**绝不用于
比赛**。窗口左下角会显示 `LEARNING BUILD — NOT COMPETITION ELIGIBLE` 水印。

学习版默认在后台无窗口运行，并默认开放完整的、同曝光的目标/能量机关真值。运行时能力文件和
`release.json` 均声明：`distribution_profile=learning`、
`competition_eligible=false`、`future_truth_included=false`。

## 一分钟启动

```bash
./install-linux.sh
daedalus-learning start --scene shooting-range
daedalus-learning status
daedalus-learning scene energy
daedalus-learning frame
daedalus-learning truth
```

默认启动和显式 `--performance` 都在后台运行，不创建图形窗口，也不需要 X11、Wayland
或 Xvfb。终端关闭后进程继续运行，使用 `daedalus-learning stop` 停止。
高性能模式仍保留离屏相机渲染、SDK/TCP 图像、同曝光真值和远程控制。

```bash
# 显式高性能模式（等同于默认启动）
daedalus-learning start --performance

# 需要人工观察时，先停止现有实例再开启窗口
daedalus-learning stop
daedalus-learning start --visible
```

可视模式中，窗口获得焦点后可用 `W/A/S/D` 移动、左 `Shift` 加速、方向键或右键拖动
控制云台、`Space` 发射；靶场的 `Q/E` 转底盘，能量机关的 `Q/E` 切换小/大能量机关。

`1.4.0-learning-r2` 修复了 r1 将 `performance` 误判为可视模式的问题。
SDK 保持 `1.4.0-learning-r1`，TCP v1 / SHM v7 / ABI revision 2 不变。

## 同曝光真值 SDK

学习版只复用 Talos v7 的既有 GroundTruth ABI 和
`TalosMetadataReader::readGroundTruthForFrame`，不创建旁路 TCP/UDP 真值协议。每个结果
只从最近 16 个已经曝光的历史槽读取；不会包含未来状态、未来控制命令或未来轨迹。

```cpp
#include <daedalus_sim_sdk/contest_client.hpp>
#include <daedalus_sim_sdk/talos_metadata_reader.hpp>

using namespace daedalus::sim::sdk::v1;

ContestClient simulator({"/tmp/daedalus-learning-1000"});
if (!simulator.connect()) return 1;
auto frame = simulator.nextFrame();

TalosMetadataMapping mapping;
mapping.open("/tmp/daedalus-learning-1000/talos_ipc_meta");
auto reader = mapping.reader();
auto truth = reader.value->readGroundTruthForFrame(
    frame.value->image.header.source_sequence);
if (!truth ||
    truth.value->producer_epoch != frame.value->image.header.producer_epoch ||
    truth.value->ground_truth.timestamp_ns !=
        frame.value->image.header.capture_timestamp_ns ||
    truth.value->exposure_state.timestamp_ns !=
        frame.value->image.header.capture_timestamp_ns) return 2;

for (std::uint32_t i = 0; i < truth.value->ground_truth.target_count; ++i) {
  const auto& target = truth.value->ground_truth.targets[i];
  // target_id, world_quaternion_wxyz, velocity, vyaw and four armor slots.
}
```

真值包括目标中心/完整姿态、线速度与角速度、目标 ID、装甲板槽位/几何/可见性，以及当前
能量机关模式、状态、转角、转向和扇叶状态。`RuntimeCapabilities` 可在连接后确认
`distribution_profile == "learning"`、`competition_eligible == false`、
`online_ground_truth_enabled == true`、`future_truth_included == false`。

要关闭在线真值而保留学习版图像和控制，启动前执行：

```bash
DAEDALUS_LEARNING_TRUTH=0 daedalus-learning start
```

关闭后真值计数为零，但同曝光姿态历史仍保留，便于使用同一 SDK 程序验证图像同步。
