# Daedalus 模拟器 SDK 1.0

SDK 是模拟器对自瞄和打符开放的唯一源码接口。消费者不得复制 Talos
共享内存结构、图像尺寸或 ABI 常量，也不得直接依赖模拟器内部 Rust 模块。

固定契约：

- SDK：`1.0.0`
- Talos SHM：`v7`
- 图像：RGB `1440×1080`
- 元数据区域：`76992` 字节
- 消费者：自瞄与打符共用同一个 `DaedalusSimSdk::DaedalusSimSdk`
- ABI：`talos_v1.hpp`
- TCP 图像协议：`tcp_image_v1.hpp`
- 端点与默认端口：`endpoints_v1.hpp`

构建、测试和安装：

```bash
cmake -S sdk/cpp -B build/sim-sdk -DCMAKE_BUILD_TYPE=Release
cmake --build build/sim-sdk --parallel
ctest --test-dir build/sim-sdk --output-on-failure
cmake --install build/sim-sdk --prefix build/sim-sdk-install
```

消费者配置：

```bash
cmake -S <consumer> -B <build> \
  -DCMAKE_PREFIX_PATH=<simulator>/build/sim-sdk-install
```

消费者按需包含：

```cpp
#include <daedalus_sim_sdk/talos_v1.hpp>
#include <daedalus_sim_sdk/tcp_image_v1.hpp>
#include <daedalus_sim_sdk/endpoints_v1.hpp>
#include <daedalus_sim_sdk/talos_metadata_reader.hpp>
#include <daedalus_sim_sdk/tcp_image_client.hpp>
#include <daedalus_sim_sdk/udp_gimbal_client.hpp>
#include <daedalus_sim_sdk/scene_control_client.hpp>
```

接口变更由模拟器仓库独立发布。消费者只更新 SDK 版本并运行契约测试，
不再维护分支私有 IPC 布局。

## 客户端能力

- `TalosMetadataMapping`：跨 Windows/Linux 只读映射 `talos_ipc_meta`，启动时校验 magic、SHM 版本、分辨率、元数据大小和 ABI revision。
- `TalosMetadataReader`：稳定快照读取图像身份、五路位姿、相机内参、底盘观测、运行状态、最新真值及按曝光帧查询的真值历史。
- `TcpImageClient`：连接 `5602/tcp`，校验完整帧头和 payload，并以 latest-only 方式向慢消费者交付最新完整图像。
- `UdpGimbalClient`：向 `5601/udp` 发送统一云台/发射命令；高性能配置默认启用接收端，无需键盘授权。
- `SceneControlClient`：向 `5603/udp` 发送带 `command_id/session_id` 的管理命令，并等待包含应用帧和错误状态的 ACK。

## 场景控制示例

```cpp
using namespace daedalus::sim::sdk::v1;

SceneControlOptions options;
options.session_id = "collection-001";
SceneControlClient control(options);
auto created = control.createSession();
auto scene = control.setScene(SceneMode::ShootingRange);

RangeTargetMotion motion;
motion.target = 3;
motion.mode = RangeMotionMode::LinearAndSpin;
motion.linear_speed_mps = 1.5F;
motion.linear_span_m = 8.0F;
motion.spin_deg_s = 45.0F;
auto applied = control.setRangeTargetMotion(motion);
```

控制状态固定为 `ok / invalid_request / unsupported / not_ready / internal_error`。除 `ping` 和 `create_session` 外，会话不匹配一律失败关闭。场景切换与 reset 只有在 Bevy 主线程完成重建后才返回 `ok`。
