# Daedalus 1.3.1-contest（Linux x86_64）

这是实验室内部算法比赛版本，只支持 Linux x86_64。它从 Linux 1.3.1 发行版继承
渲染、物理、相机和 ABI，但运行时只开放两个地图：**靶场**与**大能量机关**。普通场、
前哨场和小能量机关会被发行二进制拒绝。

## 一分钟开始

解压 `linux-x86_64.tar.gz` 后运行：

```bash
./install-linux.sh
daedalus-contest start --scene shooting-range
daedalus-contest status
```

使用同一个用户会话切换到大能量机关：

```bash
daedalus-contest scene large-energy
daedalus-contest frame
daedalus-contest aim 0 90 --fire
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
- 大能量机关中 `Q/E`：分别切换为顺时针/逆时针旋转；两个机关面保持相反方向的配对旋转。
- 方向键或按住鼠标右键移动鼠标：控制云台。
- `Space`：按射击冷却连续发射。

竞赛版启动时默认关闭自动瞄准，以保证云台由上述手动输入控制；SDK 的图像、云台命令、
发射和场景控制接口仍然可用。窗口内不再显示未向比赛开放的截图、相机、自动瞄准、
面板或地图功能键提示。

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

用 CMake 接入安装包中的 SDK：

```cmake
find_package(DaedalusSimSdk 1.3.1 REQUIRED CONFIG)
target_link_libraries(my_algorithm PRIVATE DaedalusSimSdk::DaedalusSimSdk)
```

`daedalus-contest-client` 是使用同一 SDK 的命令行验收工具。完整底层接口及固定 ABI
参见 `sdk/README.md` 和 `docs/sdk-contract.json`。
