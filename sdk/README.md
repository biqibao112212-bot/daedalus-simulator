# Daedalus Contest SDK 1.3.1-contest-r3（Linux x86_64）

实验室发行版仅交付模拟器二进制；SDK 是消费者使用模拟器的受支持接口。SDK 不包含、
管理或加载任何自瞄推理模型。

该比赛版本运行时只支持靶场与能量机关（小符与大符）。首选入口是 C++17
`ContestClient`；它将图像、按帧同步云台姿态、云台控制和受限场景切换组合成一个对象。
普通场和前哨场由发行二进制拒绝；能量机关同时支持小符和大符。

## 受支持的公开能力

- `ContestClient`：比赛首选单入口；切换靶场/能量机关、读取曝光同步图像并发送云台命令。
- `SceneControlClient`：底层受限场景控制；竞赛构建只接受靶场、能量机关及其受限场景状态。
- `getBigRuneScore(RuneTeam)`：读取红/蓝大符当前或最近一次激活的有效灯臂数、平均环数和最近命中环数；这是只读数据，不提供任意状态或真值修改。
- `getLatestArmorHit()`：读取最近一次**有效车体装甲板命中**；使用单调递增的 `event_id` 轮询新事件，不提供未命中、漏弹位置或未命中装甲信息。
- `TcpImageClient`：读取默认 RGBA32、1440×1080、latest-only 相机图像；以帧头格式为准。
- `TalosMetadataMapping` / `TalosMetadataReader`：只读固定相机内参和实际云台状态。
- `UdpGimbalClient`：发送云台绝对角命令；每条命令自动分配 `command_id`。
- `readGimbalState()`：读取最新实际角度、角速度、状态位和最后已应用命令号。
- `readGimbalStateForFrame()`：按图像 `source_sequence` 读取曝光时的实际云台角。
- `readExposureStateForFrame()`：按帧读取曝光时底盘、云台和相机位姿。
- `readRuntimeCapabilities()`：读取 wgpu 实际选择的 GPU、后端和驱动。

不提供相机标定 setter，也不提供检测框、PnP、轨迹预测或识别结果上传接口。这些结果
属于自瞄消费者自身。发布版不发布 ground truth。

## 固定契约

- SDK：`1.3.1-contest-r3`（CMake ABI 兼容版本 `1.3.1`）
- Talos SHM：`v7`，ABI revision `2`
- 默认 TCP 图像：RGBA32 `1440×1080`；旧 SHM 图像槽：RGB24
- TCP 图像：`127.0.0.1:5602`
- UDP 云台：`127.0.0.1:5601`
- UDP 场景控制：`127.0.0.1:5603`（Scene Control v2）
- 固定标定：发布根目录 `camera-calibration.json`
- GPU 运行信息：`$TALOS_IPC_DIR/daedalus-runtime-capabilities-v1.json`
- 离线 exact-corner：默认关闭；公共 schema 为
  `schemas/offline-exact-corners-v1.schema.json`，完整说明见
  `docs/OFFLINE_EXACT_CORNER_EXPORT_ZH.md`
- 离线全帧采集：默认关闭；Release collector 仅在传入
  `--save-rgba-frames --until-eof` 时写入每个 TCP identity 的 RGBA32 原始帧，
  manifest schema 为 `schemas/offline-frame-capture-v1.schema.json`；它不是 SDK
  在线真值接口。

标定文件和 `readCameraInfo()` 均为只读。最终实机标定完成后，必须更新
`calibration_id/revision` 和发布版本，不得在运行时修改。

## 大符环数读取

`ContestClient::getBigRuneScore(RuneTeam)` 返回指定红/蓝能量机关面的当前（或刚结束）
大符规则周期统计。该接口只在**大符规则驱动**状态下累计有效命中：命中当前亮起的
扇叶时，模拟器使用物理碰撞接触点在靶面局部坐标中的径向距离计算环数。有效检测直径为
300 mm；比赛版采用可复现的十个等宽 15 mm 径向环带，中心为 `10` 环、外缘为 `1` 环。

```cpp
#include <daedalus_sim_sdk/contest_client.hpp>

using namespace daedalus::sim::sdk::v1;

auto red = simulator.getBigRuneScore(RuneTeam::Red);
if (red) {
    const BigRuneScore& score = *red.value;
    // score.activated_arms: 本轮有效击中的亮扇叶数
    // score.average_ring:   本轮平均环数；尚未命中时为 0
    // score.last_ring:      最近一次命中的 1–10 环；尚未命中时为 0
    // score.last_radius_mm: 最近一次命中点距靶心的径向距离
    // score.last_target:    最近命中的 0–4 扇叶编号；尚未命中时为 -1
}
```

`run_id` 在新的大符激活周期开始时递增，`run_active` 表示当前是否仍在该激活周期。
规则超时或完成后的零值是正常重置，而不是 SDK 读取失败。竞赛客户端在能量机关选择
大符时，也会把同一份数据直接显示在底部命中统计行 `pct` 的右侧：
`big-rune R arms=… avg=… last=… B arms=… avg=… last=…`。切换到小符或离开能量机关
时该 HUD 片段隐藏；SDK 查询接口仍可用于只读诊断。

## 装甲板命中读取

`ContestClient::getLatestArmorHit()` 是比赛版的只读命中反馈。只有物理系统已经把弹丸
判定为完整尺寸车体装甲板有效命中后，才会产生新的 `event_id`；弹丸碰到车体、擦过
装甲板外沿或未命中时不会生成事件。使用者应保存上一次 `event_id`，仅在其增加时处理
新的命中。

```cpp
auto hit = simulator.getLatestArmorHit();
if (hit && hit.value->has_hit && hit.value->event_id > last_event_id) {
    last_event_id = hit.value->event_id;
    // 已确认命中的装甲板标识与累计有效命中数。
    std::cout << hit.value->target_name << ' '
              << hit.value->target_label << ' '
              << hit.value->accurate_count;
}
```

结果包含目标名称、队伍、装甲规格/标签、分类、可选弹丸追踪 ID 和累计有效命中数。它不
提供目标位姿、可见性、未命中信息或任何可用于构建目标真值的旁路数据。命令行等价入口：
`daedalus-contest armor-hit`。

## 云台坐标

- `yaw_deg`：绝对偏航角，单位度，正方向与模拟器云台局部 +Y 旋转一致。
- `pitch_deg`：绝对俯仰命令，水平为 `90°`；模拟器内部角为
  `(pitch_deg - 90°)`，并按固定俯仰限位 clamp。
- UDP 采用 latest-wins；超过 250 ms 的命令不会继续应用。
- `send()` 成功只代表数据报已交给操作系统。通过
需要跟踪命令时使用 `sendTracked()` 获取实际分配的 `command_id`，再通过
`readGimbalState().last_applied_command_id` 判断命令是否已经被模拟器应用。
图像算法应使用 `readGimbalStateForFrame(image.header.source_sequence)` 获取曝光同步角度；
历史保留最近 16 个曝光帧，过期帧会返回失败状态。

## CMake

```bash
cmake -S sdk/cpp -B build/sim-sdk -DCMAKE_BUILD_TYPE=Release
cmake --build build/sim-sdk --parallel
ctest --test-dir build/sim-sdk --output-on-failure
cmake --install build/sim-sdk --prefix build/sim-sdk-install
```

```cmake
find_package(DaedalusSimSdk 1.3.1 REQUIRED)
target_link_libraries(my_consumer PRIVATE DaedalusSimSdk::DaedalusSimSdk)
```

## 最小使用示例

```cpp
#include <daedalus_sim_sdk/runtime_capabilities.hpp>
#include <daedalus_sim_sdk/talos_metadata_reader.hpp>
#include <daedalus_sim_sdk/udp_gimbal_client.hpp>

using namespace daedalus::sim::sdk::v1;

UdpGimbalClient gimbal;
UdpGimbalCommand command;
command.yaw_deg = 15.0F;
command.pitch_deg = 92.0F;
auto sent = gimbal.send(command);

auto gpu = readRuntimeCapabilities(ipc_directory);
auto state = metadata_reader.readGimbalState();
```

底层协议常量仅用于 SDK 兼容和诊断。直接构造 UDP/共享内存不属于受支持用法，也不会
获得兼容性保证。
