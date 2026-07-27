# 模拟器任务板

上下文版本：`CTX-SIM-REPO-2026.07-v2`

## 已完成

- 统一 1440×1080 RGB 基线、250 Hz 物理和最高 200 Hz 采集目标。
- 发布 `DaedalusSimSdk 1.0.0 / SHM v7`，Rust 与 C++ 布局由静态断言和测试约束。
- SDK 支持图像身份、相机参数、曝光位姿、底盘观测、目标/打符真值、运行状态和云台命令。
- B 分支已删除本地手写布局并通过已安装 SDK 构建。
- Windows/WSL 的 1440×1080 默认图像数据面确定为 TCP；元数据、真值和控制仍使用 SDK IPC。
- Release 纯模拟器及 B+TensorRT 联合性能已实测，详见 `SIMULATOR_PERFORMANCE.md`。
- 模拟器已拆为独立 Git 仓库，具备版本文件、发布契约、SDK 安装、哈希清单和 ZIP 打包脚本。
- 正式 Release `1.0.0` 已生成并完成 76 项 SHA256 全量校验；发布目录和 ZIP 均位于工作区 `releases/daedalus-simulator/`。
- 2026-07-18 已在无游戏、保留桌面后台的环境重新实测：高性能纯模拟器主更新/采集均值 177.199/160.009 Hz，可视复测 158.298/153.015 Hz，高性能 + 自瞄 B/TensorRT 完整视觉均值 121.233 Hz；精确环境、范围、异常样本和哈希见 `SIMULATOR_PERFORMANCE.md` 与 `benchmarks/1.0.0/performance-2026-07-18.json`。
- SDK 已由协议头升级为可安装 C++17 静态客户端库，覆盖元数据映射/稳定读取、TCP 图像、UDP 云台和带 ACK 的场景控制。
- 场景控制 v1 已接入场景切换/reset、1/3 号靶车运动和打符状态；高性能配置默认保持 UDP 云台命令入口可用。
- 已建立消费者需求审批门禁：自瞄/打符调试提出的模拟器 bug 或新需求，在用户针对具体提案明确批准前只能只读诊断，禁止修改模拟器、SDK、发布脚本和正式 Release。
- 2026-07-18 可视验收复现 Windows DX12 交换链重配置崩溃：`ResizeBuffers failed: window is in use`，继而 `Surface::configure / Invalid surface`。经用户批准后，发布启动器已将可视模式默认后端隔离为 Vulkan；无窗口高性能模式继续使用 DX12，显式 `-RenderBackend` 仅供诊断。
- 模拟器 `1.0.1` 已完成正式构建与本地发布包生成。Vulkan 可见靶场 + B/TensorRT 连续运行到 119 秒，60 秒视觉均值 108.13 Hz、预览 59.95 Hz；DX12 高性能复测主更新/采集均值 168.25/152.26 Hz，均无采集丢帧和 GPU map 错误。
- 模拟器 `1.0.2` 已从清洁提交 `d7551dd` 正式发布：高性能模式禁用 Winit 并保持纯后台运行，`MainWindowHandle=0`；模拟器内部强制 20 Hz 最大物理射频，即使请求 200 Hz，250 Hz 固定步长下实测仍为 19.2308 Hz、逐发间隔固定 52 ms。SDK 保持 `1.0.0`。

## 后续

- 适配装甲板主线、火控和打符分支时，只升级 SDK 依赖，不复制模拟器代码。
- 后续正式版本继续使用 `scripts/package-release.ps1` 生成，并要求版本标签、发布清单和消费者锁文件一致。
