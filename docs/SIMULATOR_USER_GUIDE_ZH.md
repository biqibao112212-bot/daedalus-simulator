# Daedalus 模拟器使用手册（初版）

## 1. 产品目的

Daedalus 为自瞄开发者提供可重复运行的模拟环境。用户只获得模拟器二进制、场景资源和
C++ SDK，不需要模拟器 Rust 源码，也不需要把自己的检测或推理模型交给模拟器。

SDK 1.1 支持构建完整的自瞄闭环：

1. 获取带帧号和曝光时间戳的相机图像；
2. 获取固定相机标定；
3. 按图像帧号读取曝光时的实际云台角和位姿；
4. 用户自行完成检测、PnP、目标跟踪、弹道和预测；
5. 向模拟器发送云台绝对角、目标距离和开火建议；
6. 读取云台最新实际状态和最后已应用命令号；
7. 通过 SDK 切换装甲板、能量机关、前哨站和靶场场景。

模拟器不提供检测框、PnP 结果或轨迹预测结果上传接口。这些数据属于用户自瞄程序内部。

## 2. 支持范围

| 项目 | 契约 |
| --- | --- |
| 模拟器 | 1.1.0 |
| SDK | 1.1.0 |
| 操作系统 | Windows x86_64、Linux x86_64 |
| 图像 | 默认 TCP RGBA32，1440×1080，latest-only；帧头也支持 RGB24 |
| 元数据 | Talos SHM v7，ABI revision 2 |
| 图像端口 | TCP `127.0.0.1:5602` |
| 云台端口 | UDP `127.0.0.1:5601` |
| 场景控制 | UDP `127.0.0.1:5603` |

模拟器不携带 CUDA、TensorRT、ONNX、engine、checkpoint 或用户推理模型。GPU 仅用于
wgpu 渲染；用户自瞄使用什么推理框架和 GPU 版本由用户程序自行决定。

## 3. 发布包结构

```text
daedalus-simulator/
├─ bin/                         模拟器可执行文件和运行库
├─ assets/                      场景资源
├─ sdk/
│  ├─ include/                  SDK 公共头文件
│  ├─ lib/                      当前操作系统的 SDK 库
│  └─ lib/cmake/                CMake package
├─ docs/                        文档与 SDK 契约
├─ camera-calibration.json      固定相机标定
├─ platform-matrix.json         平台/GPU 支持矩阵
├─ release.json                 Release 契约
├─ release-manifest.json        文件大小和 SHA256
└─ start-simulator.ps1/.sh      启动器
```

正式包不会包含 `.rs`、`.cpp`、Cargo 工程、PDB 或可编辑模拟器 TOML。

## 4. 启动模拟器

每个发布包根目录都有 `README_ZH.md`。首次使用建议先按其中的依赖检查和安装步骤操作：
Windows 可双击 `setup.cmd`。Linux 推荐解压 `linux-x86_64.tar.gz` 后运行
`./install-linux.sh`；备用 ZIP 需要先为脚本补充可执行权限。两者都默认安装到当前用户
目录，不要求管理员权限。已经熟悉目录结构的用户也可以不安装，直接便携运行。

### 4.1 Windows

```powershell
.\start-simulator.ps1
```

显示窗口用于人工验收：

```powershell
.\start-simulator.ps1 -Visible
```

指定 IPC 目录或渲染后端：

```powershell
.\start-simulator.ps1 -IpcDir D:\sim-runtime\talos-ipc -RenderBackend dx12
.\start-simulator.ps1 -Visible -RenderBackend vulkan
```

### 4.2 Linux

```bash
./start-simulator.sh
./start-simulator.sh --visible
./start-simulator.sh --ipc-dir /tmp/daedalus/talos-ipc
```

Linux 使用 Vulkan。Windows 高性能模式默认 DX12，可视模式默认 Vulkan。wgpu 在所选
后端中自动选择高性能 GPU，用户无需选择 NVIDIA/AMD/Intel 专用 SDK。

启动后，实际 GPU 信息写入：

```text
$TALOS_IPC_DIR/daedalus-runtime-capabilities-v1.json
```

## 5. 在自瞄工程中引入 SDK

```cmake
find_package(DaedalusSimSdk 1.1 REQUIRED CONFIG)
target_link_libraries(my_autoaim PRIVATE DaedalusSimSdk::DaedalusSimSdk)
```

配置时指定模拟器 SDK：

```bash
cmake -S . -B build -DCMAKE_PREFIX_PATH=<模拟器发布目录>/sdk
cmake --build build --parallel
```

所有接口位于：

```cpp
namespace daedalus::sim::sdk::v1
```

返回值统一使用 `ClientStatus` 或 `ClientResult<T>`。调用者必须检查返回值，失败时读取
`status.error` 和 `status.message`，不得继续使用空的 `value`。

## 6. 完整自瞄闭环示例

```cpp
#include <daedalus_sim_sdk/talos_metadata_reader.hpp>
#include <daedalus_sim_sdk/tcp_image_client.hpp>
#include <daedalus_sim_sdk/udp_gimbal_client.hpp>

using namespace daedalus::sim::sdk::v1;

TalosMetadataMapping mapping;
auto opened = mapping.open(ipc_directory + "/talos_ipc_meta");
if (!opened) return 1;

auto reader_result = mapping.reader();
if (!reader_result) return 2;
auto reader = *reader_result.value;

TcpImageClient images;
if (!images.connect()) return 3;

UdpGimbalClient gimbal;
std::uint64_t previous_sequence = 0;

while (true) {
  auto frame = images.waitForLatest(previous_sequence,
                                    std::chrono::milliseconds(1000));
  if (!frame) continue;
  previous_sequence = frame.value->header.source_sequence;

  // 与这张图严格对应的曝光时云台角，而不是调用时的“最新角度”。
  auto exposure_gimbal =
      reader.readGimbalStateForFrame(frame.value->header.source_sequence);
  if (!exposure_gimbal) continue;  // 处理太慢时，16 帧历史可能已经覆盖。

  // 用户实现：RGB 图像 -> 检测 -> PnP -> 跟踪/预测 -> 弹道。
  const AimResult aim = run_user_autoaim(
      frame.value->payload,
      frame.value->header.width,
      frame.value->header.height,
      frame.value->header.capture_timestamp_ns,
      exposure_gimbal.value->yaw_deg,
      exposure_gimbal.value->pitch_deg);

  UdpGimbalCommand command;
  command.yaw_deg = aim.absolute_yaw_deg;
  command.pitch_deg = aim.absolute_pitch_deg;
  command.distance_m = aim.distance_m;
  command.fire_advice = aim.fire;
  auto sent = gimbal.sendTracked(command);
  if (!sent) continue;

  // 可选：读取最新实际状态和最后应用的 command_id。
  auto actual = reader.readGimbalState();
}
```

图像传输是 latest-only：消费者落后时，中间图像会被主动丢弃，永远交付最新完整帧，
不会交付半帧。曝光历史保存最近 16 帧，因此收到图像后应立即查询同步云台状态。

## 7. SDK 接口总览

本节用于快速查找。每个函数的签名、参数、返回值、错误处理和独立示例见
[`SDK_API_REFERENCE_ZH.md`](SDK_API_REFERENCE_ZH.md)。

### 7.1 `TcpImageClient`——相机取图

头文件：`tcp_image_client.hpp`

- `connect()`：连接模拟器图像服务器。
- `close()`：关闭连接和接收线程。
- `connected()`：查询连接状态。
- `latest()`：立即读取当前最新完整帧。
- `waitForLatest(after_source_sequence, timeout)`：等待比指定帧号更新的图像。

`TcpImageFrame::header` 主要字段：

- `source_sequence`：模拟器曝光帧号，也是按帧查询元数据的键。
- `capture_timestamp_ns`：曝光时间戳，单位纳秒。
- `producer_epoch`：模拟器进程代次；模拟器重启后会变化。
- `width / height / format / payload_bytes`：图像描述。

当前高性能 TCP 路径默认 `Rgba32`（每像素 R、G、B、A 四字节，A 可丢弃）。旧 SHM
图像槽为 `Rgb24`。用户必须依据 `header.format` 和 `payload_bytes` 解释 payload，不能
把 TCP 默认帧硬编码为三通道。

### 7.2 `TalosMetadataMapping`——打开只读元数据

头文件：`talos_metadata_reader.hpp`

- `open(metadata_path)`：只读映射 `talos_ipc_meta`。
- `close()`：解除映射。
- `isOpen()`：查询映射状态。
- `reader()`：取得 `TalosMetadataReader`。

### 7.3 `TalosMetadataReader`——标定、状态与帧同步

- `compatibility()`：检查 magic、SHM 版本、尺寸和 ABI。
- `readHeader()`：读取 producer epoch、heartbeat 和固定契约。
- `readLatestImageMeta()`：读取最新图像身份信息。
- `readCameraInfo()`：读取固定内参、畸变和图像尺寸。
- `readRuntimeState()`：读取底层最新运行状态。
- `readGimbalState()`：读取最新云台角、角速度、状态位和最后应用命令号。
- `readGimbalStateForFrame(frame_seq)`：读取指定曝光帧的实际云台角。
- `readExposureStateForFrame(frame_seq)`：读取指定帧的底盘、云台、相机世界位姿。
- `readChassisObservation()`：读取最新底盘速度、IMU 和轮速观测。
- `readLatestPose(index)`：读取低层位姿槽；新用户优先使用上述类型化接口。

开发 ABI 中保留 ground-truth 结构用于内部验收；锁定发布版只写空目标批次，不向用户
提供装甲板框、能量机关识别结果或训练标签。

### 7.4 `UdpGimbalClient`——云台和开火命令

头文件：`udp_gimbal_client.hpp`

`UdpGimbalCommand`：

- `command_id`：命令号；为 0 时 SDK 自动分配进程内递增 ID。
- `yaw_deg`：可选，绝对 yaw。
- `pitch_deg`：可选，绝对 pitch。
- `distance_m`：可选，目标距离；用于射击上下文。
- `fire_advice`：用户自瞄给出的开火建议。

`send()` 成功只表示 UDP 数据报已交给操作系统。需要关联命令时使用
`sendTracked()` 取得 SDK 实际分配的命令号，再通过
`readGimbalState().last_applied_command_id` 和实际角度确认。模拟器采用 latest-wins，
超过 250 ms 的旧命令不再持续应用。

固定弹道参数位于发布根目录 `release.json`：当前弹速 25 m/s、世界重力
`[0, -9.81, 0] m/s²`、空气阻力关闭、最小发射间隔 0.05 s。用户弹道代码应读取并锁定
对应 Release 契约，不能依赖未记录的模拟器内部常量。

### 7.5 `SceneControlClient`——场景和能量机关

头文件：`scene_control_client.hpp`

- `ping()`：连通性检查。
- `createSession()`：建立控制会话。
- `status()`：查询状态。
- `resetScene()`：重置当前场景。
- `setScene(SceneMode)`：切换 `Armor / Energy / Outpost / ShootingRange`。
- `setRangeTargetMotion()`：设置靶车静止、直线、自转或复合运动。
- `setRuneState()`：关闭、小能量机关、大能量机关，以及待激活/已激活叶片。

场景修改完成后才返回 ACK；响应包含 `command_id`、`applied_frame_seq`、时间戳、状态和
错误信息。

### 7.6 `readRuntimeCapabilities()`——GPU 运行信息

头文件：`runtime_capabilities.hpp`

返回实际产品版本、锁定状态、渲染后端、GPU 名称、vendor/device ID、设备类型、驱动和
驱动版本。该接口只描述模拟器渲染 GPU，不描述用户自瞄的 CUDA/TensorRT 环境。

### 7.7 低层兼容与协议辅助接口

以下接口是 SDK 公共实现的一部分，主要用于测试、兼容诊断或封装其他语言；普通 C++
自瞄优先使用前述类型化客户端：

- `encodeUdpGimbalCommand()`：编码云台 JSON 数据报。
- `encodeSetSceneArgs()`、`encodeRangeTargetMotionArgs()`、`encodeRuneStateArgs()`：编码
  场景参数。
- `buildSceneControlRequest()`、`parseSceneControlResponse()`：构造/解析场景控制协议。
- `tcp_image_v1.hpp`：TCP 帧头编解码、格式和 payload 大小校验。
- `talos_v1.hpp`：固定 ABI 布局、常量和兼容检查。
- `readLatestGroundTruth()`、`readGroundTruthForFrame()`：内部开发/验收兼容接口；正式锁定
  发布版目标计数固定为 0，不得作为用户检测结果来源。

## 8. 坐标系定义

### 8.1 云台命令坐标

模拟器内部采用右手系，世界 `+Y` 向上。SDK 边界统一使用度：

- 命令是相对底盘坐标的绝对云台角，不是世界绝对角。
- 水平朝向为 `yaw_deg = 0°`、`pitch_deg = 90°`。
- `yaw_deg` 绕竖直 `+Y` 轴旋转；从上方看，正 yaw 向机器人左侧转。
- `pitch_deg > 90°` 表示抬头，`pitch_deg < 90°` 表示低头。
- 内部俯仰角为 `(pitch_deg - 90°)`。
- 当前固定俯仰限位约为内部 ±45°，即 SDK 命令约 `[45°, 135°]`，越界会 clamp。
- yaw 采用最短角方向逐步跟随目标。

`readGimbalState()` 和 `readGimbalStateForFrame()` 返回同一套角度定义，用户无需自行处理
内部弧度或 90° 零位偏移。

### 8.2 相机光学坐标

相机坐标不能完全由用户任意定义，因为图像、内参和 PnP 必须共享同一坐标语义。SDK
使用常见右手光学坐标：

- `+X`：图像向右；
- `+Y`：图像向下；
- `+Z`：镜头前方；
- 像素原点：图像左上角；`u` 向右，`v` 向下；
- 长度单位：米；四元数文件顺序：`xyzw`；曝光位姿 ABI 顺序：`wxyz`。

固定内参、畸变、`T_gimbal_camera` 和数字曝光位于 `camera-calibration.json`。当前标定为
`daedalus-camera-1440x1080-v2` / revision 2，固定曝光为 `EV100=9.7`、关闭自动曝光、
`Tonemapping=None`。模拟器没有物理传感器，因此不存在可解释为真实相机快门时间或模拟
增益的值。用户可以在自己的算法内部转换坐标系，但不能改变模拟器发布契约。

## 9. 时间和同步规则

- 所有 `timestamp_ns` 均为 Unix epoch 纳秒；同一图像的 header、同步云台状态和曝光
  位姿应相等。
- `source_sequence/frame_seq` 是首选关联键，不能只按“读取顺序”猜测对应关系。
- `producer_epoch` 变化表示模拟器已经重启，消费者必须清空跟踪器和旧帧缓存。
- 控制回传和图像采集是异步的；请求角、实际角和曝光时角必须分别处理。

## 10. 用户负责的自瞄模块

模拟器 SDK 提供环境与 I/O，但以下内容由用户实现：

- 装甲板/能量机关检测与分类；
- 相机模型使用、PnP 和坐标转换；
- 目标跟踪、旋转目标建模和轨迹预测；
- 弹道补偿、延迟估计和开火策略；
- 用户自己的 CUDA、TensorRT、ONNX Runtime 或其他推理环境；
- 日志、性能统计、模型版本和失败降级。

因此 SDK 的边界是“完整自瞄所需的模拟传感器输入、同步状态、控制输出和场景控制”，
而不是替用户实现自瞄算法。

## 11. 性能简报

### 11.1 Windows 正式包短测（2026-07-26）

被测对象为模拟器 1.1.0 正式 Windows x86_64 包，源码提交 `6883879`，运行在
Intel Core i9-14900HX、RTX 4060 Laptop GPU、驱动 572.70、DX12 高性能无窗口模式。
测试使用发行包 SDK 持续读取 1440×1080 RGBA32 latest-only TCP 图像，预热 3 秒并采样
5 秒。测试时同一 GPU 正在运行不可中断的 CUDA 训练，因此本结果是并发负载下的保守
下界，不是空闲机器峰值。

| 指标 | 1.1.0 短测 | 旧 1.0.x 联合自瞄基线 | 说明 |
| --- | ---: | ---: | --- |
| SDK 实收图像 | 84.44 FPS | 121.37 FPS | 当前短测低 30.43% |
| 单帧 payload | 6,220,800 B（RGBA32） | 4,665,600 B（RGB24） | 当前每帧大 33.33% |
| payload 吞吐 | 500.94 MiB/s | 540.01 MiB/s | 当前短测低 7.24% |
| 采样帧数 | 424 / 5.02 s | 6123 / 长时间联合测试 | 短测仅用于快速筛查 |
| GPU 状态 | 训练并发，约 46%→76% | 自瞄联合负载约 31%～41% | 环境不等价 |

结论：最终包在高 GPU 并发下仍能稳定提供约 84 FPS、501 MiB/s 的真实 SDK 图像流，接口
和数据面没有失效；按字节吞吐看与旧基线接近。实验室内部发布接受这个重载下界，但它不
代表空闲机器峰值，也不证明空闲状态 FPS 与旧版相同；后续可在 CUDA 训练结束后按“预热
8 秒、连续采样 20 秒”的旧口径补测。Linux llvmpipe 仅作无独显启动验收，不承诺实时
图像性能。

### 11.2 用户侧性能建议

- 使用 latest-only，不要在自瞄进程中积压图像队列；
- 收到图像后先保存同帧云台状态，再进行耗时推理；
- 记录 `source_sequence`、时间戳、实际处理 FPS 和丢帧数；
- 需要实时自瞄时使用硬件 GPU；CPU/llvmpipe 只适合启动和接口诊断；
- FPS 下降时同时检查模拟器渲染、用户推理和其他 CUDA 进程，不能只看模拟器进程。

## 12. 常见问题

### 取到图但找不到同帧云台状态

消费者处理超过 16 个曝光帧后历史会覆盖。应缩短处理链路，收到图像后先读取并保存
同步状态，再进行耗时推理。

### `UnsupportedAbiRevision`

使用了旧 SDK。模拟器 1.1 要求 SDK 1.1、SHM v7、ABI revision 2。

### 图像延迟不断增大

不要建立无限队列。SDK 已采用 latest-only，用户内部队列也应保持 1～2 帧并丢弃旧帧。

### GPU 不兼容

先读取 runtime capabilities，确认实际 backend/adapter/driver。Windows 可在 DX12 与
Vulkan 间切换；Linux 需要系统 Vulkan loader 和正确驱动。模拟器包不安装显卡驱动。

### 用户能否修改相机内外参

不能。发布版标定是只读契约，也没有 setter。用户可在自己的程序中读取后转换坐标，
但不能改变模拟器产生图像时使用的相机模型。
