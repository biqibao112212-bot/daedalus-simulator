# 模拟器性能文档共享入口

其他自瞄、打符分支在启动或采集模拟器前，必须先读取仓库根目录的性能文档：

[SIMULATOR_PERFORMANCE.md](../../SIMULATOR_PERFORMANCE.md)

它是当前 `integration/perf-main` 模拟器分支的性能与配置基线，详细记录：

- 高性能采集模式和可视验收模式的区别；
- 高性能模式作为后续自瞄数据采集默认模式的约定；
- RTX 4060 Laptop GPU / DX12 的实测渲染、物理和 Talos 采集频率；
- Release 编译命令、工作树路径、可执行文件路径和资源路径；
- `config.performance.toml`、`config.toml` 与各环境变量的用途；
- RGB-only、200 Hz 采集、250 Hz 物理、预览关闭和 WSL bridge 的边界；
- auto-aim 接入方式、统计 JSON 和性能验收条件。

本次只提交文档入口；不要因为读取这份文档而修改 armor、energy-buff 或
fire-control 分支中的模拟器源码和配置。模拟器源码更新仍只在
`integration/perf-main` 上进行，其他分支按根目录性能文档选择运行模式即可。
