# Daedalus Simulator SDK 1.3 C++ API 函数参考

本文档面向使用模拟器完成自瞄训练的 C++ 开发者，说明每个受支持接口的用途、参数、
返回值、时序和典型调用方式。完整运行流程、坐标系和发行包说明见
[`SIMULATOR_USER_GUIDE_ZH.md`](SIMULATOR_USER_GUIDE_ZH.md)。

## 1. 支持范围

- 官方语言：C++17。
- 构建系统：CMake 3.16 或更新版本。
- 平台：Windows x86_64、Linux x86_64；两个系统使用相同 API、不同二进制库。
- 命名空间：`daedalus::sim::sdk::v1`。
- SDK 提供图像、时间戳、固定标定、云台状态、云台命令、场景控制和运行 GPU 信息。
- SDK 不提供检测框、分类、PnP、跟踪、预测、弹道解算或推理模型。
- 正式锁定版的 ground truth 数量固定为 0，不能作为自瞄输入。

## 2. 引入 SDK

用户只需安装与当前操作系统匹配的发行包，不需要 Rust、WSL 或模拟器源码。

```cmake
cmake_minimum_required(VERSION 3.16)
project(lab_autoaim LANGUAGES CXX)

find_package(DaedalusSimSdk 1.3 REQUIRED CONFIG)

add_executable(lab_autoaim main.cpp)
target_compile_features(lab_autoaim PRIVATE cxx_std_17)
target_link_libraries(lab_autoaim PRIVATE DaedalusSimSdk::DaedalusSimSdk)
```

Windows PowerShell：

```powershell
cmake -S . -B build -DCMAKE_PREFIX_PATH="C:\path\to\daedalus-simulator\sdk"
cmake --build build --config Release
```

Linux：

```bash
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_PREFIX_PATH=/path/to/daedalus-simulator/sdk
cmake --build build --parallel
```

## 3. 统一返回值和错误处理

头文件：`client_result.hpp`

### `ClientStatus`

用于没有业务返回值的操作。

```cpp
struct ClientStatus {
  ClientError error;
  std::string message;
  bool ok() const noexcept;
  explicit operator bool() const noexcept;
};
```

调用成功时 `status.ok()` 和 `static_cast<bool>(status)` 都为真；失败时从
`status.error` 获取机器可判断的错误，从 `status.message` 获取诊断文字。

### `ClientResult<T>`

用于返回业务数据的操作。

```cpp
template <typename T> struct ClientResult {
  std::optional<T> value;
  ClientStatus status;
  bool ok() const noexcept;
  explicit operator bool() const noexcept;
};
```

必须先判断结果，再访问 `value`：

```cpp
auto result = reader.readGimbalState();
if (!result) {
  std::cerr << result.status.message << '\n';
  return;
}
const GimbalState& state = *result.value;
```

常见 `ClientError` 包括参数错误、连接失败、发送/接收失败、未就绪、超时、对端关闭、
协议错误、元数据不兼容和共享内存快照不稳定。网络调用失败后不要继续使用空的 `value`。

## 4. 默认端点和 IPC 目录

头文件：`endpoints_v1.hpp`

| 功能 | 默认值 |
| --- | --- |
| TCP 图像 | `127.0.0.1:5602` |
| UDP 云台命令 | `127.0.0.1:5601` |
| UDP 场景控制 | `127.0.0.1:5603` |
| IPC 环境变量 | `TALOS_IPC_DIR` |
| 元数据文件名 | `talos_ipc_meta` |
| GPU 信息文件名 | `daedalus-runtime-capabilities-v1.json` |

发行版只监听本机回环地址。启动模拟器和自瞄程序时必须使用同一个 IPC 目录。

```cpp
const std::filesystem::path ipc_dir = /* 与启动器 -IpcDir/--ipc-dir 相同 */;
const std::string metadata_path =
    (ipc_dir / std::string(kMetaFileName)).string();
```

## 5. 相机取图：`TcpImageClient`

头文件：`tcp_image_client.hpp`、`tcp_image_v1.hpp`

### 构造函数

```cpp
explicit TcpImageClient(
    UdpEndpoint endpoint = {"127.0.0.1", kTcpImagePort});
```

不传参数时连接本机 5602 端口。构造对象不会自动连接。

### `connect()`

```cpp
ClientStatus connect();
```

连接图像服务并启动 SDK 内部接收线程。应在模拟器图像服务就绪后调用。失败时可等待后
重试；同一对象重新连接前建议先调用 `close()`。

### `close()` / `connected()`

```cpp
void close() noexcept;
bool connected() const noexcept;
```

`close()` 停止接收并关闭连接，可重复调用。`connected()` 仅表示客户端当前连接状态，
不代表一定已有图像。

### `latest()`

```cpp
ClientResult<TcpImageFrame> latest() const;
```

立即复制并返回 SDK 当前保存的最新完整帧；尚未收到帧时返回 `NotReady`。它不会等待新帧，
也不保证返回帧号比上次调用更新。

### `waitForLatest()`

```cpp
ClientResult<TcpImageFrame> waitForLatest(
    std::uint64_t after_source_sequence,
    std::chrono::milliseconds timeout) const;
```

- `after_source_sequence`：只接受帧号严格大于该值的帧；首次调用传 0。
- `timeout`：最长等待时间；超时返回 `ClientError::Timeout`。
- 返回：一份完整的 `TcpImageFrame`，其中 payload 由返回对象自己持有。

图像流是 latest-only。消费者处理过慢时会跳过中间帧，这是控制实时延迟的正常行为。

```cpp
TcpImageClient images;
if (auto status = images.connect(); !status) {
  throw std::runtime_error(status.message);
}

std::uint64_t previous = 0;
auto frame = images.waitForLatest(previous, std::chrono::milliseconds(1000));
if (frame) {
  previous = frame.value->header.source_sequence;
}
```

### `TcpImageFrame` 和帧头

```cpp
struct TcpImageFrame {
  tcp_image::FrameHeader header;
  std::vector<std::uint8_t> payload;
};
```

`FrameHeader` 的业务字段：

| 字段 | 含义 |
| --- | --- |
| `format` | `Rgb24` 或 `Rgba32`；当前默认高性能路径为 `Rgba32` |
| `width`, `height` | 图像像素尺寸 |
| `payload_bytes` | payload 实际字节数 |
| `producer_epoch` | 模拟器进程代次；变化表示模拟器重启 |
| `source_sequence` | 曝光帧号，也是查询同帧元数据的主键 |
| `capture_timestamp_ns` | 曝光时刻，Unix epoch 纳秒 |

不要把 `capture_timestamp_ns` 当作图像到达时间。模拟器重启后应在发现
`producer_epoch` 变化时清空跟踪器、旧帧号和延迟统计。

### 转为 OpenCV 图像

OpenCV 不是 SDK 依赖。若用户工程已经使用 OpenCV，可按帧头格式创建视图：

```cpp
const auto& f = *frame.value;
const int type = f.header.format == tcp_image::PixelFormat::Rgba32
                     ? CV_8UC4
                     : CV_8UC3;
cv::Mat view(static_cast<int>(f.header.height),
             static_cast<int>(f.header.width), type,
             const_cast<std::uint8_t*>(f.payload.data()));

cv::Mat bgr;
if (f.header.format == tcp_image::PixelFormat::Rgba32) {
  cv::cvtColor(view, bgr, cv::COLOR_RGBA2BGR);
} else {
  cv::cvtColor(view, bgr, cv::COLOR_RGB2BGR);
}
```

`view` 引用 `f.payload` 的内存，不能比 `TcpImageFrame` 活得更久；需要跨线程保存时使用
`clone()` 或移动整个 `TcpImageFrame`。

## 6. 打开元数据：`TalosMetadataMapping`

头文件：`talos_metadata_reader.hpp`

### `open()`

```cpp
ClientStatus open(const std::string& metadata_path);
```

以只读方式映射 `TALOS_IPC_DIR/talos_ipc_meta`。参数必须是完整文件路径，不是 IPC 目录。
模拟器尚未创建文件、权限不足或 ABI 不兼容时返回失败。

### `reader()`

```cpp
ClientResult<TalosMetadataReader> reader() const;
```

从已经打开的映射创建轻量读取器。读取器依赖 mapping 的生命周期，因此必须保证
`TalosMetadataMapping` 在读取器使用期间仍然存在。

### `close()` / `isOpen()`

```cpp
void close() noexcept;
bool isOpen() const noexcept;
```

`close()` 后由该 mapping 创建的读取器不应继续使用。

```cpp
TalosMetadataMapping mapping;
auto opened = mapping.open(metadata_path);
if (!opened) throw std::runtime_error(opened.message);

auto made_reader = mapping.reader();
if (!made_reader) throw std::runtime_error(made_reader.status.message);
TalosMetadataReader reader = *made_reader.value;
```

## 7. 元数据读取：`TalosMetadataReader`

所有读取函数都会取得一致快照；如果生产者正在反复改写且无法获得稳定快照，会返回
`UnstableSnapshot`。调用者可以在下一帧重试，不能使用半更新数据。

### `compatibility()`

```cpp
TalosCompatibility compatibility() const noexcept;
```

检查映射、magic、SHM 版本、尺寸、元数据大小和 ABI revision。正常结果是
`TalosCompatibility::Compatible`。模拟器 1.1 对应 SHM v7、ABI revision 2。

### `readHeader()`

```cpp
ClientResult<ShmHeader> readHeader() const;
```

读取创建时间、心跳、固定图像尺寸、SHM 版本和 ABI revision。可用 `heartbeat_ns` 判断
生产者是否仍在更新。

### `readLatestImageMeta()`

```cpp
ClientResult<ImageMeta> readLatestImageMeta() const;
```

读取最新图像的帧号、时间戳、尺寸和底层 buffer ID。普通自瞄取图应使用
`TcpImageClient`；该函数主要用于状态诊断和帧身份交叉检查。

### `readCameraInfo()`

```cpp
ClientResult<CameraInfo> readCameraInfo() const;
```

返回固定的 `fx/fy/cx/cy`、五项畸变系数和图像尺寸。SDK 没有修改标定的 setter。
发行根目录的 `camera-calibration.json` 还包含固定外参、数字曝光和标定版本，用户应将
版本写入日志。数字曝光固定为 `EV100=9.7`，自动曝光关闭且不做色调映射；物理快门和
模拟增益不适用于数字渲染器。

### `readGimbalState()`

```cpp
ClientResult<GimbalState> readGimbalState() const;
```

读取调用时最新的实际云台状态：

```cpp
struct GimbalState {
  std::uint64_t frame_seq;
  std::uint64_t timestamp_ns;
  std::uint64_t last_applied_command_id;
  float yaw_deg;
  float pitch_deg;
  float yaw_velocity_deg_s;
  float pitch_velocity_deg_s;
  std::uint32_t status_flags;
};
```

该函数适合控制反馈和命令确认，不适合替代曝光时角度。

### `readGimbalStateForFrame()`

```cpp
ClientResult<GimbalState> readGimbalStateForFrame(
    std::uint64_t frame_seq) const;
```

按 TCP 图像的 `source_sequence` 查询该帧曝光时的实际云台角。自瞄 PnP、姿态补偿应使用
这个函数，而不是 `readGimbalState()`。模拟器只保留最近 16 个曝光帧；收到图像后应先
保存同帧状态，再执行耗时推理。

```cpp
auto synced = reader.readGimbalStateForFrame(
    frame.value->header.source_sequence);
if (!synced) {
  // 处理过慢、帧号错误或模拟器已重启；丢弃本帧。
  continue;
}
```

### `readExposureStateForFrame()`

```cpp
ClientResult<ExposureState> readExposureStateForFrame(
    std::uint64_t frame_seq) const;
```

读取同一曝光时刻的底盘、云台和相机世界位姿以及云台弧度角。它用于坐标变换、延迟研究
和回归测试；只做基础自瞄时优先使用更简单的 `readGimbalStateForFrame()`。

这里的“曝光状态”是图像生成时刻的位姿快照，不是光学曝光参数。固定光学曝光请读取
发行包根目录的 `camera-calibration.json`。

`ExposureState::state_flags` 指明哪些位姿有效。四元数为 `wxyz`，长度单位为米，内部角度
为弧度。不得忽略有效位直接使用未声明有效的字段。

### `readChassisObservation()`

```cpp
ClientResult<ChassisObservation> readChassisObservation() const;
```

读取底盘速度、角速度、轮速、加速度、姿态、陀螺仪和加速度计观测。当前完整自瞄的最低
要求不依赖该函数；需要运动补偿或教学实验时可选用。

### `readRuntimeState()`

```cpp
ClientResult<RuntimeState> readRuntimeState() const;
```

返回内部弧度制的最新运行状态。普通用户优先使用角度制的 `readGimbalState()`；该接口
保留给底层诊断和兼容性测试。

### `readLatestPose()`

```cpp
ClientResult<PoseMeta> readLatestPose(std::size_t pose_index) const;
```

读取底层位姿槽。SDK 定义了云台、里程计、枪口和相机索引。普通用户应优先使用同帧的
类型化接口，以避免把“最新位姿”和历史图像错误配对。

### ground-truth 兼容接口

`readLatestGroundTruth()` 和 `readGroundTruthForFrame()` 为内部开发及 ABI 兼容保留。正式
`distribution-release` 模拟器返回空目标批次，用户不得将其作为检测或识别结果来源。

## 8. 云台控制：`UdpGimbalClient`

头文件：`udp_gimbal_client.hpp`

### `UdpGimbalCommand`

```cpp
struct UdpGimbalCommand {
  std::uint64_t command_id = 0;
  std::optional<float> yaw_deg;
  std::optional<float> pitch_deg;
  std::optional<float> distance_m;
  bool fire_advice = false;
};
```

- `command_id`：传 0 时 SDK 分配进程内单调递增 ID；通常保持 0。
- `yaw_deg`：底盘局部坐标下绝对 yaw；正方向向机器人左侧旋转。
- `pitch_deg`：绝对 pitch；90° 水平，大于 90° 抬头，小于 90° 低头。
- `distance_m`：可选目标距离，单位米；用于射击上下文。
- `fire_advice`：用户算法的开火建议，不是检测结果。

只设置 yaw 或只设置 pitch 是合法的；两个角都未设置但提供其他字段也会按协议发送。
pitch 当前会被模拟器限制在约 `[45°, 135°]`，云台最大角速度 3 rad/s。

### 构造函数

```cpp
explicit UdpGimbalClient(
    UdpEndpoint endpoint = {"127.0.0.1", kUdpCommandPort});
```

默认发送到本机 5601。构造时不会建立长连接，因为 UDP 是无连接协议。

### `send()`

```cpp
ClientStatus send(const UdpGimbalCommand& command) const;
```

成功只表示数据报已交给操作系统，不表示模拟器已经执行。无需跟踪命令 ID 的简单控制可
使用该函数。

### `sendTracked()`

```cpp
ClientResult<std::uint64_t> sendTracked(
    const UdpGimbalCommand& command) const;
```

返回实际发送的 `command_id`。用 `readGimbalState().last_applied_command_id` 判断模拟器已
处理到哪条命令，并用实际角度判断机械跟随结果。

`readGimbalStateForFrame()`只回答“这张图曝光时云台在哪里”，不携带命令应用 ID；不要
用它代替 `readGimbalState()`做命令 ACK。UDP 无连接，`sendTracked()`成功也不表示远端
监听器已经启动。消费者应在收到第一帧或确认运行时能力文件就绪后开始控制循环，并持续
发送最新目标。

```cpp
UdpGimbalClient gimbal;
UdpGimbalCommand command;
command.yaw_deg = target_yaw_deg;
command.pitch_deg = target_pitch_deg;
command.distance_m = target_distance_m;
command.fire_advice = should_fire;

auto sent = gimbal.sendTracked(command);
if (!sent) {
  std::cerr << sent.status.message << '\n';
  continue;
}

auto actual = reader.readGimbalState();
if (actual && actual.value->last_applied_command_id >= *sent.value) {
  // 模拟器已处理该命令；角度可能仍在以有限速度跟随。
}
```

云台协议采用 latest-wins。控制程序应持续发送最新目标，不要排队重放旧命令；超过
250 ms 的旧控制不会继续应用。

## 9. 场景控制：`SceneControlClient`

头文件：`scene_control_client.hpp`

### 创建客户端

```cpp
SceneControlOptions options;
options.endpoint = {"127.0.0.1", kUdpSceneControlPort};
options.timeout = std::chrono::milliseconds(1000);
SceneControlClient scene(options);
```

`session_id` 可留空并通过 `createSession()` 获取；`first_command_id` 默认为 1。

### 通用响应

所有场景操作返回 `SceneControlResponse`，主要字段为：

- `command_id`：本次命令 ID。
- `session_id`：会话 ID。
- `status`：`Ok`、请求无效、不支持、未就绪或内部错误。
- `applied_frame_seq`：场景修改生效的模拟器帧号。
- `timestamp_ns`：应用时间。
- `message`：诊断信息。

`ClientResult` 成功表示收到并解析了响应；业务是否成功还必须检查
`response.status == SceneControlStatus::Ok`。

### `ping()` / `createSession()` / `status()`

```cpp
ClientResult<SceneControlResponse> ping(const std::string& args_json = "{}");
ClientResult<SceneControlResponse> createSession(
    const std::string& args_json = "{}");
ClientResult<SceneControlResponse> status(const std::string& args_json = "{}");
```

- `ping()`：检查服务是否可达。
- `createSession()`：建立控制会话。
- `status()`：读取当前场景控制状态。

### `resetScene()`

```cpp
ClientResult<SceneControlResponse> resetScene(
    const std::string& args_json = "{}");
```

重置当前场景到确定的初始状态。训练用例开始前建议调用一次。

### `setScene()`

```cpp
ClientResult<SceneControlResponse> setScene(SceneMode mode);
```

`SceneMode` 可选 `Armor`、`Energy`、`Outpost`、`ShootingRange`。

```cpp
auto response = scene.setScene(SceneMode::Energy);
if (!response || response.value->status != SceneControlStatus::Ok) {
  // 记录 ClientStatus 或响应 message。
}
```

### `setRangeTargetMotion()`

```cpp
ClientResult<SceneControlResponse> setRangeTargetMotion(
    const RangeTargetMotion& motion);
```

```cpp
RangeTargetMotion motion;
motion.target = 3;
motion.mode = RangeMotionMode::LinearAndSpin;
motion.direction_deg = 90.0F;
motion.linear_speed_mps = 1.5F;
motion.linear_span_m = 4.0F;
motion.spin_deg_s = 60.0F;
auto response = scene.setRangeTargetMotion(motion);
```

运动模式包括静止、直线、自转、直线并自转。参数是否适用于当前场景由服务端响应确认。

### `setRuneState()`

```cpp
ClientResult<SceneControlResponse> setRuneState(const RuneState& state);
```

```cpp
RuneState rune;
rune.mode = RuneMode::Small;            // Off / Small / Large
rune.pending_targets = {0, 1, 2, 3, 4};
rune.activated_targets = {};
auto response = scene.setRuneState(rune);
```

该接口替代本地键盘输入，用于开始/关闭小能量机关和大能量机关，以及设置叶片激活状态。
正式发布版用户不能通过键盘或调试 UI 绕过 SDK 修改场景。

### 字符串重载和协议辅助函数

`setScene(args_json)`、`setRangeTargetMotion(args_json)`、`setRuneState(args_json)`、
`request()` 以及 `encode*`/`parse*` 函数用于协议测试或封装其他语言。普通 C++ 用户优先
使用枚举和结构体重载，它们提供参数校验并减少 JSON 拼写错误。

## 10. GPU 信息：`readRuntimeCapabilities()`

头文件：`runtime_capabilities.hpp`

```cpp
ClientResult<RuntimeCapabilities> readRuntimeCapabilities(
    const std::string& ipc_directory);
```

参数是 IPC 目录，不是 JSON 完整路径。应在模拟器渲染器初始化后调用。返回字段包括：

- 产品版本和 `distribution_locked`；
- 适配器选择策略；
- 实际渲染后端；
- GPU 名称、vendor/device ID、设备类型；
- 驱动名称和版本信息。

```cpp
auto gpu = readRuntimeCapabilities(ipc_dir.string());
if (gpu) {
  std::cout << gpu.value->render_backend << ": "
            << gpu.value->adapter_name << '\n';
}
```

它描述的是模拟器渲染 GPU，不代表用户自瞄程序的 CUDA、TensorRT 或 ONNX Runtime 配置。

## 11. 推荐的完整自瞄调用顺序

```cpp
TalosMetadataMapping mapping;
if (auto s = mapping.open(metadata_path); !s) return 1;
auto made_reader = mapping.reader();
if (!made_reader) return 2;
auto reader = *made_reader.value;

TcpImageClient images;
if (auto s = images.connect(); !s) return 3;
UdpGimbalClient gimbal;

std::uint64_t previous = 0;
std::uint64_t producer_epoch = 0;
for (;;) {
  auto frame = images.waitForLatest(previous, std::chrono::milliseconds(1000));
  if (!frame) continue;

  if (producer_epoch != 0 &&
      producer_epoch != frame.value->header.producer_epoch) {
    reset_user_tracker();
  }
  producer_epoch = frame.value->header.producer_epoch;
  previous = frame.value->header.source_sequence;

  auto exposure_gimbal = reader.readGimbalStateForFrame(previous);
  if (!exposure_gimbal) continue;

  // 用户实现：图像 -> 检测 -> PnP -> 跟踪/预测 -> 弹道。
  AimResult aim = run_user_autoaim(
      frame.value->payload,
      frame.value->header,
      *exposure_gimbal.value);

  UdpGimbalCommand command;
  command.yaw_deg = aim.absolute_yaw_deg;
  command.pitch_deg = aim.absolute_pitch_deg;
  command.distance_m = aim.distance_m;
  command.fire_advice = aim.fire;
  auto sent = gimbal.sendTracked(command);

  // 可选：读取最新实际状态，做命令确认和控制日志。
  auto actual = reader.readGimbalState();
}
```

每帧至少记录 `producer_epoch`、`source_sequence`、`capture_timestamp_ns`、曝光时云台角、
发送的 `command_id/yaw/pitch` 和最新实际角度。这样才能定位识别、延迟、坐标变换还是控制
执行造成的偏差。

## 12. 坐标、单位和时序速查

- 所有 SDK 云台命令和 `GimbalState` 使用度；底层 `RuntimeState/ExposureState` 使用弧度。
- yaw/pitch 命令是底盘局部坐标下的绝对角，不是角速度或增量。
- 水平零位为 `yaw=0°`、`pitch=90°`。
- 相机光学坐标：`+X` 图像向右、`+Y` 图像向下、`+Z` 镜头前方。
- 像素原点在左上角，`u` 向右、`v` 向下。
- 距离和位置单位为米，速度为米每秒。
- `timestamp_ns` 为 Unix epoch 纳秒；帧关联优先使用 `source_sequence/frame_seq`。
- 图像、曝光状态和控制反馈是异步数据，不能用调用先后顺序推测同帧关系。

## 13. 不建议直接使用的接口

以下内容虽然因 ABI、测试或未来语言封装而位于公开头文件中，但不是普通自瞄程序的首选：

- 手工调用 `encodeUdpGimbalCommand()` 或构造场景 JSON；
- 手工解析 TCP wire header；
- 直接映射旧 RGB24 图像池；
- 用 `readLatestPose()` 代替按帧查询；
- 使用 ground-truth 结构作为检测结果；
- 绕过 SDK 自行实现共享内存/TCP/UDP 协议。

SDK 的版本兼容保证以这里记录的类型化 C++ 接口为边界。需要其他语言时，应在独立项目中
基于 SDK 做受控绑定，并固定 SDK 版本，不应复制协议常量后长期自行维护。
