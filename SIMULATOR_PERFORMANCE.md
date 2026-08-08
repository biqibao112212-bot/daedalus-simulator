# Daedalus Simulator 1.2.1 构建、运行与性能基线

- 适用仓库/分支：`daedalus-simulator/main`
- 本机固定目录：`D:\仿真\repos\daedalus-simulator`
- 当前正式 Windows Release：`D:\仿真\releases\daedalus-simulator\1.2.1\windows-x86_64`
- 公共 SDK：`DaedalusSimSdk 1.2.0`，`SHM v7 ABI r2`

本文是模拟器性能配置和公开基线的权威文档。当前 Windows Release 1.2.1 的机器可读基线见
[`benchmarks/1.2.1/performance-release.json`](benchmarks/1.2.1/performance-release.json)；旧版 Linux/WSL 数据仅作历史参考，不能作为当前 Windows 原生自瞄 B 采集口径。

## 当前 Windows 原生采集基线（1.2.1，2026-08-08）

所有新的自瞄 B 采集必须使用 Windows Release 模拟器、Windows 原生 TensorRT 桥、localhost TCP RGBA32 1440×1080 与任务专用 `D:\仿真\runtime` 目录。禁止使用 WSL、`/mnt/d` 文件映射、文件三缓冲轮询或 Debug 模拟器作为默认链路或性能证据。

| 范围 | 条件 | 结果 |
| --- | --- | ---: |
| 模拟器源/渲染/采集 | Release、DX12、无预览、TCP、无消费者 | `main_update_hz` 177.951 Hz；`capture_copy_submit_hz` 176.951 Hz |
| 自瞄 B 完整链路 | Windows 原生桥、Windows TensorRT、Stage3 JSONL、靶场目标 3 连续真值云台锁定、自转 114.592°/s | 4,229 帧/30 s，即 140.967 FPS；Stage3 4,970 行 |

完整链路的源采集间隔 p50/p95/p99 为 6.526/12.810/23.906 ms，桥完成间隔为 6.100/15.700/27.180 ms。靶场 TCP 原始帧确认目标 3 与装甲板位于视野内。原始帧、逐帧 JSONL、分布图、日志索引与复现命令保存于 `D:\仿真\runtime\SHOOTING_RANGE_TARGET_IN_VIEW_PERFORMANCE_1.2.1_20260808.md`。当前 engine 对该渲染目标仍返回零接受检测；这限制检测准确率结论，但不影响可见目标下的采集吞吐结论。

## 1.1.1 Linux RTX 3090 验收（2026-07-30）

Linux x86_64 RC2 完整包（实现提交 `5bd4a56`）在 AutoDL Ubuntu 22.04、Xeon Gold 6330、
RTX 3090、驱动 580.76.05 上通过验收。服务器没有 `DISPLAY`、Wayland 或 Xvfb；Vulkan
使用 NVIDIA `libEGL_nvidia.so.0` 无显示 ICD。发布包经 GitHub Release 签名地址和 AutoDL
加速通道下载，SHA256 校验后使用包内安装器安装，安装后二进制与包内文件逐字节一致。

发行 SDK 消费者预热 3 秒并连续采样 20.007 秒：收到 723 帧 1440×1080 RGBA32 图像，
实收 36.138 FPS、214.393 MiB/s，TCP 超时为 0。722 次同帧云台查询全部与图像时间戳
一致；最新云台状态确认命令 ID 1 已应用，场景 `ping/create_session/status` 均取得 ACK。
一帧同帧历史查询在采样瞬间未取得稳定快照，消费者按接口约定丢弃该帧。

GPU 54 个样本的利用率平均 13.185%、最大 24%，显存最大 947 MiB、功耗平均 85.27 W；
模拟器进程约使用 329% CPU，因此该服务器结果主要受 Xeon 单核/物理调度约束，不是
RTX 3090 的渲染上限。它比下方 Windows/i9/RTX 4060 并发短测低约 57.2%，但两台机器的
CPU、系统、后端和并发负载都不同，不能据此推导 Linux 比 Windows 固定慢 57.2%。本结果
证明的是该完整 Linux 包在真实 NVIDIA GPU 上可稳定完成自瞄数据闭环，不是跨平台峰值
排行榜。

## 1.1.0 发布候选短测（2026-07-26）

Windows x86_64 正式包（source `6883879`）在 RTX 4060 Laptop / DX12 上通过发行 SDK
持续读取 1440×1080 RGBA32 图像。预热 3 秒、采样 5.021 秒，共取得 424 帧：实收
84.44 Hz、payload 500.94 MiB/s。测试期间同卡正在运行不可中断的 CUDA 训练，GPU 利用率
从采样前约 46% 到采样后约 76%，因此这是并发负载下界，不是空闲性能基线。

旧 1.0.x 自瞄联合基线为 RGB24 121.37 Hz，约 540.01 MiB/s。1.1.0 短测的字节吞吐低
7.24%，但 FPS 低 30.43%；同时 1.1.0 RGBA32 单帧比旧 RGB24 大 33.33%。数据面在重载下
保持稳定，但当前条件不足以签署“空闲 FPS 与旧版相同”。机器可读结果见
[`benchmarks/1.1.0/performance-short-2026-07-26.json`](benchmarks/1.1.0/performance-short-2026-07-26.json)。

实验室内部发布接受该重载下界，不再把空闲 GPU 复测设为本次发布门禁。若后续需要形成
可横向比较的正式性能基线，仍使用下文旧口径：停止其他 GPU 计算任务，预热 8 秒，连续
采样 20 秒。

## 固定基线

- 当前 TCP 图像：RGBA32，`1440×1080`，单帧 6,220,800 字节；
- 物理控制时基：250 Hz，当前 Avian 配置每步 8 个 substep；
- 高性能采集上限：200 Hz；
- Windows 渲染后端：高性能模式使用 DX12；可视验收模式使用 Vulkan；
- Linux 渲染后端：Vulkan；高性能模式不需要显示服务器；
- Windows/Linux 图像数据面：TCP 5602，latest-only；
- 元数据、曝光位姿和真值：SDK IPC；
- 云台命令：UDP 5601；场景控制：UDP 5603；
- 只使用 Release 构建测性能，禁止用 Debug 帧率代替。

200 Hz 是配置上限，不是承诺帧率。实际吞吐由渲染、GPU readback、TCP、消费者推理和机器后台负载共同决定。

## 构建和正式打包

```powershell
Set-Location D:\仿真\repos\daedalus-simulator
cargo build --release --features talos
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\package-release.ps1
```

开发构建位于 `target\release\daedalus.exe`。日常运行和消费者验证必须使用
`D:\仿真\releases\daedalus-simulator\<version>`；SDK 与 Release 规则见 `RELEASE.md`。

## 两种运行模式

### 默认：高性能模式

```powershell
Set-Location D:\仿真\releases\daedalus-simulator\1.2.1\windows-x86_64
powershell -NoProfile -ExecutionPolicy Bypass -File .\start-simulator.ps1
```

该模式设置 `DAEDALUS_PERF_DISABLE_UI=1`，不创建主窗口、不拉起靶场前端
debug 子进程，但离屏 Talos 相机仍持续渲染、readback 并向消费者发布图像。
没有前端窗口不代表没有图像采集；判断采集是否正常应读取
`capture_copy_submit_hz`、`capture_processing_complete_total` 和消费者输入计数。

### 可视验收模式

```powershell
Set-Location D:\仿真\releases\daedalus-simulator\1.2.1\windows-x86_64
powershell -NoProfile -ExecutionPolicy Bypass -File .\start-simulator.ps1 -Visible
```

该模式默认选择 Vulkan 并启用最高 60 Hz 的可见预览，只用于人工检查画面、相机和场景。默认采集仍是离屏 1440×1080 图像，不应把 `preview_present_hz` 当成采集帧率。

启动器也接受显式后端参数，供诊断使用：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\start-simulator.ps1 -Visible -RenderBackend vulkan
```

不要把 `-Visible -RenderBackend dx12` 作为正式验收配置。本机 Windows 11、RTX 4060 Laptop、驱动 572.70 上，DX12 可见交换链在窗口重配置时可复现 `ResizeBuffers failed: window is in use`，随后 Bevy 报 `Surface::configure / Invalid surface` 并退出；同一 Release 改为 Vulkan 后，靶场切换、运动靶、1440×1080 离屏采集和 TensorRT 消费链路均正常。该问题不影响默认无窗口 DX12 高性能模式。

## 自瞄 B + TensorRT

从消费者仓库启动：

```powershell
Set-Location D:\仿真\repos\aim-stack
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\run-autoaim-b.ps1
```

当前联合基线使用正式模拟器 Release 和 Windows 原生自瞄 B。TensorRT 属于消费者推理后端，不由模拟器自动加载；消费者应使用其 Windows engine 与原生 `aim_sim_windows_auto_aim_bridge.exe`。WSL 联合链路仅为历史资料，不能作为新的自瞄 B 采集入口。

## 2026-07-18 当前实测

### 1.0.1 可见模式修复验收

`1.0.1` 仅改变发布启动器的渲染后端选择，不改变模拟器二进制逻辑、SDK 或 IPC。正式候选包完成以下回归：

| 模式 | 采样 | 主更新 Hz | 采集/TCP Hz | 物理 Hz | 预览 Hz | B 完整视觉 Hz | 端到端平均 / P95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Vulkan 可见靶场 + B/TensorRT | 60 s | 108.12 | 108.12 / 108.12 | 250.00 | 59.95 | 108.13 | 32.46 / 41.82 ms |
| DX12 高性能纯模拟器 | 20 s | 168.25 | 152.26 / 不适用 | 250.01 | 0 | 未运行 | 不适用 |

可见联合测试运行到 119.137 秒时，主窗口和独立自瞄调试窗口均保持响应；完成 9509 个视觉结果，后端为 `vivsionn_trt`，曝光真值严格匹配。两项测试的采集队列丢帧、GPU map 错误均为 0；联合测试 TCP 读写错误也为 0。靶场通过 SDK ACK 确认切换完成，3 号靶车按 1.5 m/s 往复平移和 45°/s 自转。

### 被测版本和环境

| 项目 | 值 |
| --- | --- |
| 模拟器 Release | `1.0.0` / tag `simulator-v1.0.0` / source `5ce9369e83a289edd8c5969c1dfd7ae784fccb94` |
| 模拟器二进制 SHA256 | `5E20FA25356E50F6ADBC2E904A208D007396ED5B86DA9737AFB2EDE7BC8BAB6B` |
| 自瞄消费者提交 | `946f90fa52eae577c5bca9b1d202127d10242e9a` |
| 装甲模型 | FP16 TensorRT，输入 `1×3×640×640` |
| 模型 SHA256 | `BF3DB6F2F9A6D71371E1D82CC9F03467EC9B3BD71E0549689E410469E2B839D4` |
| CPU / 内存 | Intel Core i9-14900HX，24C/32T；32 GiB |
| GPU | NVIDIA GeForce RTX 4060 Laptop GPU，8188 MiB，驱动 572.70 |
| 系统 / 电源计划 | Windows 11 Pro build 26200；`Codex Gaming Stability Test` |
| 渲染 / 推理工具链 | DX12；GCC 11.4；CMake 3.22.1；CUDA 12.8；TensorRT 10.9.0.34 |

测试前没有游戏、模拟器或自瞄进程。保留用户桌面程序时，10 次空载 GPU 采样为 29%–33%、显存 2584–2597 MiB、功耗 9.90–10.05 W，因此本结果不是“绝对净空机器最佳值”，而是该后台状态下的可复现工作基线。

### 测量口径

- 纯模拟器：预热 8 秒，之后每秒读取一次 `DAEDALUS_STATS_JSON`，连续 20 秒；
- 联合模式：桥接器已处理 100 帧后再预热 15 秒，之后连续采样 30 秒；
- 表中主值为采样窗口均值，括号内为中位数；范围为每秒统计的最小值到最大值；
- `main_update_hz` 是模拟器主更新频率，旧字段 `render_fps` 只是它的兼容别名；
- `capture_copy_submit_hz` 是离屏图像 GPU copy 提交率；
- `tcp_image_sent_hz` 是有 TCP 消费者时的实际图像发送率；
- `completed_vision_rolling_hz` 是自瞄完整视觉结果滚动频率；
- 联合模式的 `pipeline_latency_mean_ms` 是运行至该采样点的累计流水线均值，不是 P99。

### 稳定结果

| 模式 | 主更新 Hz | 离屏采集提交 Hz | TCP 图像 Hz | 物理 Hz | 可见预览 Hz | B 完整视觉 Hz | 流水线均值 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 高性能，纯模拟器，无 TCP 消费者 | 177.199 (180.070) | 160.009 (160.366) | 不适用 | 250.022 (249.914) | 0 | 未运行 | 不适用 |
| 可视验收，纯模拟器，历史 DX12 复测 | 158.298 (158.751) | 153.015 (152.501) | 不适用 | 250.009 (249.932) | 60.010 (59.827) | 未运行 | 不适用 |
| 高性能，模拟器 + 自瞄 B/TensorRT | 121.565 (122.913) | 121.333 (121.921) | 121.366 (121.921) | 249.963 (249.859) | 0 | 121.233 (122.000) | 6.032 ms |

每秒统计范围：

| 模式 | 主更新 | 采集/TCP | 完整视觉 | GPU 利用率 | GPU 功耗 |
| --- | ---: | ---: | ---: | ---: | ---: |
| 高性能纯模拟器 | 138.399–191.365 Hz | 138.399–169.763 Hz | 不适用 | 27%–44% | 14.96–19.17 W |
| 可视验收历史 DX12 复测 | 144.915–169.771 Hz | 142.917–159.539 Hz | 不适用 | 31%–43% | 15.13–17.97 W |
| 高性能 + 自瞄 B | 106.648–135.964 Hz | TCP 106.648–133.979 Hz | 106–132 Hz | 31%–41% | 25.30–34.20 W |

联合运行最终计数：自瞄处理 6133 帧、完成 6122 个视觉结果；模拟器发送 6123 帧；曝光真值严格匹配；采集队列丢帧 0；GPU map 错误 0。桥接器后端日志为 `vivsionn_trt`，UDP 云台输出保持启用。

第一轮可视测试中出现过一次 7.073 Hz 的单秒停顿，使该轮主更新均值降到 153.376 Hz；立即复测后 20 秒范围稳定为 144.915–169.771 Hz。机器可读文件同时保留第一轮和复测摘要，不隐藏异常样本。

### 历史结果为什么不同

旧实验曾得到纯模拟器约 199.79 Hz，以及 B + TensorRT 约 140.81/139.41 Hz。它们使用了不同的文件/TCP组合、统计窗口或后台负载，只保留为历史最佳参考，不能替代上表当前基线。尤其自瞄 B 不得使用文件三缓冲图像模式：旧 Windows/WSL 长测只有约 0.49 个有效输入/秒；当前完全 Windows 链路固定使用 TCP。

## 复现检查表

1. 两个仓库都必须为文档记录的提交或其明确后继，并保持 `git status --short` 为空；
2. 校验 Release 二进制与模型 SHA256；
3. 使用 1440×1080、Release、TCP、250 Hz 和 200 Hz capture cap；高性能模式用 DX12，可视验收模式用 Vulkan；
4. 记录电源计划，并在启动前用 `nvidia-smi` 连续采样空载 GPU；
5. 设置统计输出后启动：

```powershell
$env:DAEDALUS_STATS_JSON='D:\仿真\runtime\perf\simulator.json'
$env:DAEDALUS_P1_TIMING='1'
```

6. 不得用可见预览帧率、主更新兼容别名或“配置上限 200 Hz”冒充实际图像吞吐；
7. 联合测试必须同时报告模拟器 TCP 发送、自瞄完整视觉结果、流水线延迟、丢帧和曝光匹配；
8. 原始日志可在结论固化后删除，受 Git 跟踪的 JSON 摘要和本文是长期证据。
