# Daedalus 模拟器性能与运行配置

本文档是当前模拟器性能运行的唯一复现说明。其他自瞄、打符分支在
需要启动模拟器、采集图像或比较帧率时，先读取本文档；本次只更新文档，
不对这些分支的模拟器源码、配置或自瞄逻辑做同步修改。

## 1. 固定工作树、分支和路径

当前已验收的模拟器集成分支：

```text
分支：integration/perf-main
工作树：D:\仿真\worktrees\main-performance
```

所有命令都必须从工作树根目录执行。资源路径、`config.toml` 和相对路径
都以该目录为基准：

```powershell
Set-Location D:\仿真\worktrees\main-performance
$env:BEVY_ASSET_ROOT = (Get-Location).Path
```

Release 可执行文件路径：

```text
D:\仿真\worktrees\main-performance\target\release\daedalus.exe
```

不要使用 `target\debug\daedalus.exe` 评价帧率。Debug 构建会显著降低
渲染、GPU readback 和 Talos 处理速度。

## 2. 两种运行模式

### 2.1 高性能采集模式（今后自瞄数据默认使用）

用途：自瞄图像采集、Talos/IPC 数据流、性能基准和批量仿真数据。

入口配置：`config.performance.toml`。

固定策略：

| 项目 | 设置 | 说明 |
| --- | --- | --- |
| 物理固定频率 | `fixed_hz = 250` | 4 ms 控制/物理时间基准 |
| Avian 子步数 | `substep_count = 1` | 当前高吞吐配置；提高后必须重新测量 |
| RGB 分辨率 | `1280x720` | 当前集成分支的采集尺寸 |
| RGB-only | `DAEDALUS_TALOS_RGB_ONLY=1` | 关闭 depth/dataset readback，保留颜色图像 |
| 采集上限 | `DAEDALUS_TALOS_CAPTURE_MAX_HZ=200` | 目标为 5 ms 一个采集槽，不追赶突发帧 |
| 预览 | `preview.enabled = false` | 不渲染可视窗口的 preview pass |
| UI/诊断 | `DAEDALUS_PERF_DISABLE_UI=1` | 关闭 Egui、Inspector、诊断日志 |
| 阴影/FXAA | `false/false` | 关闭额外 GPU 开销 |
| Present mode | `immediate` | 不等待 VSync |
| WGPU | `dx12` + `high` | 使用独立 GPU 高性能电源策略 |
| 受管 WSL 桥接 | `managed_bridge_enabled = false` | 基准时不自动拉起 TensorRT/WSL 进程 |

高性能模式仍然会渲染离屏 Talos 相机并发布 RGB、位姿和时间戳；黑色的
可视窗口只表示 preview 被关闭，不表示图像采集失败。

高性能模式下自瞄的两种连接方式：

1. 保持 `managed_bridge_enabled = false`，由外部已经运行的算法/桥接进程
   连接 Talos IPC。这是测量模拟器本身性能时的默认方式。
2. 复制一份性能配置并将 `managed_bridge_enabled` 改为 `true`，让模拟器
   管理 WSL/TensorRT 进程。此方式会引入算法启动和通信开销，必须单独测量，
   不得把它的数值和纯模拟器基线混在一起。

高性能自瞄启动命令：

```powershell
Set-Location D:\仿真\worktrees\main-performance
$env:BEVY_ASSET_ROOT = (Get-Location).Path
$env:WGPU_BACKEND = 'dx12'
$env:WGPU_POWER_PREF = 'high'
$env:DAEDALUS_CONFIG = 'config.performance.toml'
$env:DAEDALUS_PERF_DISABLE_UI = '1'
$env:DAEDALUS_TALOS_RGB_ONLY = '1'
$env:DAEDALUS_TALOS_CAPTURE_MAX_HZ = '200'
$env:DAEDALUS_AUTO_AIM_ON_START = '1'
$env:DAEDALUS_STATS_JSON = 'artifacts/performance/latest.json'
New-Item -ItemType Directory -Force artifacts/performance | Out-Null
.\target\release\daedalus.exe
```

### 2.2 可视验收模式（只用于看画面）

用途：确认场景、相机、目标和光照正常；不作为最高采集帧率基线。

该模式仍使用高性能配置作为底层配置，但通过环境变量打开 preview：

```powershell
Set-Location D:\仿真\worktrees\main-performance
$env:BEVY_ASSET_ROOT = (Get-Location).Path
$env:WGPU_BACKEND = 'dx12'
$env:WGPU_POWER_PREF = 'high'
$env:DAEDALUS_CONFIG = 'config.performance.toml'
$env:DAEDALUS_PREVIEW_ENABLED = '1'
$env:DAEDALUS_PREVIEW_MAX_HZ = '60'
$env:DAEDALUS_TALOS_RGB_ONLY = '1'
$env:DAEDALUS_TALOS_CAPTURE_MAX_HZ = '200'
$env:DAEDALUS_AUTO_AIM_ON_START = '0'
Remove-Item Env:DAEDALUS_PERF_DISABLE_UI -ErrorAction SilentlyContinue
.\target\release\daedalus.exe
```

此模式会同时运行可视 preview 相机和离屏采集相机，因此帧率低于高性能
模式是正常的。当前 RTX 4060 Laptop GPU / DX12 实测约为：

| 指标 | 可视验收实测 |
| --- | ---: |
| 渲染帧率 | `约 147 FPS` |
| preview 显示 | `约 60 FPS` |
| Talos/RGB 采集 | `约 145 FPS` |
| 物理 | `约 250 Hz` |
| capture queue drops | `0` |
| GPU map errors | `0` |

如果只是确认“能否正常渲染”，使用该模式；如果要采集自瞄数据，切回
2.1 高性能模式。

## 3. 已验证性能基线

测试硬件和后端：NVIDIA RTX 4060 Laptop GPU、DX12、Windows。
测试程序为优化 Release 构建，工作树为 `integration/perf-main`。

### 高性能模式实测

采样时长为启动稳定后的约 36.33 秒，auto-aim subscription 开启，受管
WSL bridge 关闭：

| 指标 | 结果 |
| --- | ---: |
| Render FPS | `203.15` |
| Physics step rate | `249.95 Hz` |
| Talos RGB/publish | `197.17 Hz` |
| Capture queue drops | `0` |
| Fast-buffer drops | `0` |
| GPU map errors | `0` |
| Publisher lock drops | `1` 次非阻塞锁竞争 |

因此当前默认性能目标写成：渲染至少 150 FPS，物理保持约 250 Hz，
RGB/Talos 采集接近 200 Hz。当前基线已达到约 200 FPS；任何后续修改都
必须重新生成统计文件并更新本节，而不能沿用旧数值。

## 4. 编译方法

每次测量前都使用 Release 编译：

```powershell
Set-Location D:\仿真\worktrees\main-performance
cargo fmt --all -- --check
cargo check --features talos
cargo test --features talos
cargo build --release --features talos
```

编译成功后使用：

```text
D:\仿真\worktrees\main-performance\target\release\daedalus.exe
```

如果切换了 Bevy、WGPU、分辨率、物理子步、capture driver 或 Talos IPC，
必须重新执行完整 Release 测量。不要用 Debug 结果和本节基线比较。

## 5. 统计输出和验收方法

设置 `DAEDALUS_STATS_JSON` 后，模拟器约每 100 ms 覆盖写入 JSON：

```powershell
$r = Get-Content artifacts/performance/latest.json -Raw | ConvertFrom-Json
$r.render_fps
$r.physics_step_hz
$r.talos_frame_fps
$r.capture_copy_submit_hz
$r.capture_queue_drop_total
$r.capture_fast_no_buffer_drop_total
$r.capture_fast_map_error_total
```

高性能模式的最低验收条件：

1. `render_fps >= 150`。
2. `physics_step_hz` 接近 `250`。
3. `talos_frame_fps` 或 `capture_copy_submit_hz` 接近 `200`。
4. queue drop、fast-buffer drop 和 map error 不应持续增长。
5. 采集图像内容仍需单独检查目标是否在视野内；帧率通过不等于数据质量通过。

## 6. 配置边界和默认使用规则

- 今后涉及自瞄训练、轨迹预测、Talos 数据或性能对比，默认使用
  `config.performance.toml` 和 2.1 命令。
- 涉及画面、目标位置、相机方向和灯光的人工验收，使用 2.2 命令。
- `config.toml` 是互动地图配置，不作为高性能基准；不要为了跑一次基准
  直接覆盖它。
- 高性能模式关闭 depth/dataset 通道，只适合颜色图像和运行时自瞄验证。
  需要深度或完整数据集时必须建立独立测量记录。
- 本次只写文档，没有把高性能模式强行改成其他分支的代码默认值；
  “默认使用高性能模式”是后续启动和测量的项目约定。
