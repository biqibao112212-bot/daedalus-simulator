# 模拟器公共 SDK 接口

- 契约版本：`DaedalusSimSdk 1.0.0`
- 生产仓库：`D:\仿真\repos\daedalus-simulator`
- 生产分支：`main`
- IPC：`SHM v7`
- 图像：RGB24，最大且默认 `1440×1080`

消费者应从模拟器安装 SDK，不得复制 `talos_v1.hpp`：

```powershell
Set-Location D:\仿真\repos\daedalus-simulator
.\scripts\build-release.ps1
```

CMake 消费方式：

```cmake
find_package(DaedalusSimSdk 1 REQUIRED CONFIG)
target_link_libraries(my_consumer PRIVATE DaedalusSimSdk::DaedalusSimSdk)
```

开发时可把 `D:\仿真\repos\daedalus-simulator\build\sim-sdk-install` 加入 `CMAKE_PREFIX_PATH`；正式消费者必须使用 Release 包内的 `sdk` 目录。

## 契约内容

`daedalus::sim::sdk::v1` 公开版本常量、固定图像上限和以下 ABI：

- `ShmHeader`：magic、版本、结构大小、生产者 epoch。
- 三槽 `ImageMeta`：图像序号、曝光时间戳、尺寸、缓冲槽。
- `CameraInfo` 与位姿三缓冲：相机内参及曝光匹配位姿。
- `ChassisObservation`：消费者可用的底盘观测。
- `GroundTruthBatch`、打符真值和 16 槽曝光历史：仅用于标注与验收，严禁作为学习模型输入。
- `RuntimeState`：跟随状态和运行时云台状态。
- `GimbalCommand`：自瞄/打符共用的云台与发射命令出口。
- `tcp_image_v1.hpp`：TCP 图像帧头、像素格式、大小校验和编解码。
- `endpoints_v1.hpp`：IPC 文件名、环境变量和默认网络端口。

消费者打开 IPC 后必须调用 `isCompatible`；不兼容时立即失败，不得猜测布局。所有跨流关联必须同时校验生产者 epoch、`frame_seq` 和 `timestamp_ns`，不得用相邻帧代替缺失曝光。

## 传输

- 元数据、真值、运行状态、命令：文件支持的 SDK IPC，目录由 `TALOS_IPC_DIR` 明确指定。
- 图像性能模式：`DAEDALUS_TALOS_IMAGE_TRANSPORT=tcp`，默认监听 `0.0.0.0:5602`；消费者使用 latest-only TCP 接收器。
- 文件图像模式：只作兼容与同系统调试。1440×1080 跨 Windows/WSL 会因读取期间槽位更新而大量拒帧，不得作为默认配置。
