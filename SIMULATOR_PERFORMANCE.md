# Daedalus Simulator 1.0.0 构建、运行与性能

适用分支：`integration/perf-main`
工作目录：`D:\仿真\worktrees\main-performance`
SDK：`DaedalusSimSdk 1.0.0`，`SHM v7`
固定基线：RGB24 `1440×1080`、物理 250 Hz、Release

## 构建

```powershell
Set-Location D:\仿真\worktrees\main-performance
cargo build --release --features talos
```

Release 程序：`D:\仿真\worktrees\main-performance\target\release\daedalus.exe`。禁止用 Debug 帧率代表性能。

SDK 安装方法见 `agent-team/SIMULATOR_INTERFACE.md`，默认本机安装路径为 `D:\仿真\worktrees\main-performance\build\sim-sdk-install`。

## 默认高性能模式

该模式关闭可见预览，但离屏 Talos 相机仍会渲染并发布图像；窗口黑屏不等于没有采集。

```powershell
$env:BEVY_ASSET_ROOT='D:\仿真\worktrees\main-performance'
$env:WGPU_BACKEND='dx12'
$env:WGPU_POWER_PREF='high'
$env:DAEDALUS_CONFIG='config.performance.toml'
$env:DAEDALUS_PERF_DISABLE_UI='1'
$env:DAEDALUS_TALOS_RGB_ONLY='1'
$env:DAEDALUS_TALOS_CAPTURE_MAX_HZ='200'
$env:DAEDALUS_TALOS_IMAGE_TRANSPORT='tcp'
$env:DAEDALUS_AUTO_AIM_ON_START='1'
Set-Location D:\仿真\worktrees\main-performance
.\target\release\daedalus.exe
```

只有单进程/同系统兼容调试才把图像传输改为 `file`。B 分支跨 Windows/WSL 必须使用 `tcp`。

## 可视验收模式

在上述配置基础上删除 `DAEDALUS_PERF_DISABLE_UI`，并设置：

```powershell
$env:DAEDALUS_PREVIEW_ENABLED='1'
$env:DAEDALUS_PREVIEW_MAX_HZ='60'
```

可视模式只用于确认画面、相机和场景，不作为最高吞吐基线。

## 2026-07-18 实测

硬件/后端：RTX 4060 Laptop GPU、DX12；样本均为当前 Release、1440×1080。

| 模式 | 模拟器 FPS | RGB 输出 | 物理 | B 完整视觉结果 | 平均流水线延迟 |
| --- | ---: | ---: | ---: | ---: | ---: |
| 纯模拟器，文件图像兼容模式 | 199.79 | 186.80 Hz | 249.74 Hz | 未运行 | 不适用 |
| B + TensorRT，TCP 图像模式 | 140.81 | 137.81 Hz | 249.65 Hz | 139.41 Hz | 7.84 ms |

B 联合实测共处理 4003 帧，滚动完整视觉结果约 137 Hz，最新源图到完成约 28.62 ms；模拟器采集队列丢帧 0、GPU map 错误 0。测试结束时主动停止消费者产生一次 TCP connection reset，这是测试终止行为，不是运行中故障。

文件图像跨 Windows/WSL 的长测只有约 0.49 个有效输入/秒，原因是 4.67 MB 图像读取期间生产者更新三缓冲槽；因此它已被明确排除出 B 默认配置。
