# Daedalus 模拟器常见故障与安全恢复手册

- 文档版本：`TROUBLESHOOTING-PUBLIC-2026.07-v1`
- 适用范围：Daedalus Simulator `1.0.x`、DaedalusSimSdk `1.0.0`
- 固定基线：Windows 11、RGB24 `1440×1080`、高性能模式 DX12、可视验收模式 Vulkan
- 公共端点：云台命令 `5601/udp`、TCP 图像 `5602/tcp`、场景控制 `5603/udp`

本文是模拟器所有消费者共享的公共排障资源。自瞄、火控和打符任务可以读取本文，
但不得因此修改模拟器源码、SDK、发布脚本或正式 Release。消费者调试发现疑似模拟器
缺陷或新需求时，必须先按文末模板向用户提交证据；只有用户针对该提案明确批准后，
才能在模拟器仓库中实施修改。

正式运行、接口和场景语义分别以 `SIMULATOR_PERFORMANCE.md`、
`SIMULATOR_INTERFACE.md`、`SCENARIO_CONTROL.md` 和 Release 内的
`sdk-contract.json` 为准。本文只处理故障定位，不替代版本契约。

## 首先确认你运行的是什么

排障前先记录 Release、模式和端口所有者。不要使用 Debug 构建的表现替代正式 Release，
也不要在不知道进程来源时直接全局杀进程。

```powershell
$release = 'D:\仿真\releases\daedalus-simulator\1.0.3'
Get-Content -Raw -LiteralPath "$release\release.json" | ConvertFrom-Json |
    Select-Object product,version,sdk_version,shm_version,tcp_image_port,scene_control_port

Get-CimInstance Win32_Process |
    Where-Object Name -eq 'daedalus.exe' |
    Select-Object ProcessId,ParentProcessId,ExecutablePath,CommandLine

Get-NetTCPConnection -LocalPort 5602 -ErrorAction SilentlyContinue |
    Select-Object LocalAddress,LocalPort,State,OwningProcess
Get-NetUDPEndpoint -LocalPort 5603 -ErrorAction SilentlyContinue |
    Select-Object LocalAddress,LocalPort,OwningProcess
```

推荐的隔离顺序是：

1. 只启动正式模拟器 Release；
2. 确认渲染、采集统计和端口；
3. 通过正式 SDK 完成 `ping/create_session` 和所需场景 ACK；
4. 再启动 TCP 图像消费者；
5. 最后启动 TensorRT、PnP、tracker 或打符算法。

这样可以区分模拟器、网络/WSL、消费者启动器和算法本身的故障。

## 故障速查

| 症状 | 最可能原因 | 首要检查 | 安全处理 |
| --- | --- | --- | --- |
| 高性能模式窗口全黑或没有窗口 | 这是预期行为，UI 被关闭 | 统计文件中的采集计数和消费者输入计数 | 不要为了采集强制开启可见窗口 |
| 可视模式启动后黑屏或窗口崩溃 | 错误地使用了可见 DX12，或资源仍在加载 | 启动日志中的 backend、`ResizeBuffers`、`Invalid surface` | 使用正式启动器 `-Visible`，让其默认选择 Vulkan |
| `talos_image_transport=tcp_bind_failed` | 旧 `daedalus.exe` 占用 5602 | TCP 5602 的 `OwningProcess` 和进程路径 | 只终止已确认属于本次测试的旧进程；未知进程则停止并上报 |
| Scene Control 完全无 ACK | 5603 未监听、WSL 使用了错误 host、启动尚未就绪 | UDP 5603、绑定地址、WSL 默认路由 | Windows 绑定 `0.0.0.0:5603`；WSL 连接默认路由的 Windows host |
| `create_session` 成功，`set_scene` 超时 | 场景重建期间过早发送后续命令，或重复创建会话 | ACK 时间、session_id、命令重试层数 | 等待模拟器就绪；同一 SDK 会话执行有界重试，不嵌套无限重试 |
| TCP 连接数/发送数始终为 0 | 消费者未启动、提前退出或连接了错误地址 | 模拟器 TCP 统计、消费者退出码和日志 | 先验证 5602 监听，再单独启动 SDK TCP 客户端 |
| 跨 Windows/WSL 图像极慢或大量拒帧 | 误用了文件映射图像数据面 | `DAEDALUS_TALOS_IMAGE_TRANSPORT` | `1440×1080` 跨系统固定使用 TCP latest-only |
| 关闭 PowerShell 后下次启动端口仍占用 | wrapper 退出但子 `daedalus.exe` 或 Linux bridge 成为孤儿 | Windows 进程树、WSL `ps`、5602/5603 | 启动器必须拥有并精确清理自己创建的进程；禁止 broad `pkill` |
| 中文工作目录在 WSL 中变成乱码或路径丢反斜杠 | Windows PowerShell 5 输出编码或 shell 转义 | 打印转换前后的绝对路径 | UTF-8 解码 `wslpath` 输出，传入前把 `\` 规范化为 `/` |
| 短时探针没有任何视觉结果 | 首次 CMake/TensorRT 构建耗尽了测试窗口 | 构建时间、bridge 首次输出时间 | 先完成构建，再开始计时；不要把 0 帧写成性能结论 |
| 模拟器正常但日志没有 TensorRT | TensorRT 属于消费者，不由模拟器自动启动 | 消费者 backend 日志和 engine 路径 | 由消费者启动器显式启用，并验证实际 backend 为 TensorRT |
| 看见靶场但靶车不运动 | 只切换了地图，未成功应用运动命令 | `set_scene` 与 `set_range_target_motion` 的 ACK | 仅通过 SDK 控制 1/3 号靶车，以 ACK 判断成功 |
| 看到约 60 FPS 就认为采集不足 | 把可见预览帧率当成离屏采集帧率 | `preview_present_hz` 与 `capture_copy_submit_hz` | 分别报告预览、采集、TCP 和消费者完整视觉帧率 |

## 1. 高性能模式没有画面

默认高性能模式设置 `DAEDALUS_PERF_DISABLE_UI=1`。可见预览关闭，但离屏 Talos
相机仍然执行 `1440×1080` 渲染、GPU readback 和 TCP 发布，所以“看不见窗口”不能
证明“没有采集”。

```powershell
$env:DAEDALUS_STATS_JSON='D:\仿真\runtime\diagnostics\simulator.json'
Set-Location D:\仿真\releases\daedalus-simulator\1.0.3
powershell -NoProfile -ExecutionPolicy Bypass -File .\start-simulator.ps1
```

运行后检查：

```powershell
$stats = Get-Content -Raw 'D:\仿真\runtime\diagnostics\simulator.json' |
    ConvertFrom-Json
$stats | Select-Object talos_image_transport,capture_copy_submit_hz,`
    capture_processing_complete_total,talos_tcp_image_connected,`
    talos_tcp_image_sent_hz,capture_queue_drop_total,capture_fast_map_error_total
```

无消费者时 `talos_tcp_image_connected` 和发送帧率可以是 0；但采集提交、处理完成计数
应持续增长。连接消费者后还应看到 TCP 连接和发送计数增长。

需要人工看画面时才使用：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\start-simulator.ps1 -Visible
```

正式启动器会在可视模式选择 Vulkan。不要把 `-Visible -RenderBackend dx12` 作为默认
配置；已验证机器上它可能触发 `ResizeBuffers`，随后出现 `Invalid surface`。

## 2. 5602/5603 被占用或出现 `tcp_bind_failed`

已验证的一类失败是：测试 wrapper 先退出，真正的 `daedalus.exe` 仍在后台运行，导致
下一实例不能绑定 TCP 5602；同一残留实例也可能占用 Scene Control 5603，使日志和 ACK
来自不同实例，造成“场景偶尔成功但图像始终为 0”的假象。

先查所有者：

```powershell
$owners = @(
    Get-NetTCPConnection -LocalPort 5602 -ErrorAction SilentlyContinue |
        Select-Object -ExpandProperty OwningProcess
    Get-NetUDPEndpoint -LocalPort 5603 -ErrorAction SilentlyContinue |
        Select-Object -ExpandProperty OwningProcess
) | Sort-Object -Unique

foreach ($ownerPid in $owners) {
    Get-CimInstance Win32_Process -Filter "ProcessId=$ownerPid" |
        Select-Object ProcessId,ParentProcessId,Name,ExecutablePath,CommandLine
}
```

只有在路径、命令行和启动时间都证明该进程属于当前测试时，才能停止它：

```powershell
Stop-Process -Id <confirmed-pid>
```

不得把“启动前全局杀所有 daedalus/wsl/bridge”当成修复。可靠启动器应在启动前对端口
失败关闭，打印 PID/进程名；退出时只清理自己创建的 Windows 进程树和带唯一 token 的
Linux bridge。关闭 Windows 侧 `wsl.exe` 并不保证 Linux 子进程已经退出。

每次探针结束后至少验证：

```powershell
Get-Process daedalus -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 5602 -ErrorAction SilentlyContinue
Get-NetUDPEndpoint -LocalPort 5603 -ErrorAction SilentlyContinue
wsl.exe -d Ubuntu-OSTEP -- bash -lc `
  "ps -eo pid,ppid,args | grep -E 'aim_sim_talos_auto_aim_bridge' | grep -v grep || true"
```

修复启动器后必须连续运行两次相同探针；只成功一次不能证明生命周期问题已经解决。

## 3. WSL 场景控制无 ACK

Windows 模拟器默认的回环地址不能直接当作 WSL 中的 `127.0.0.1` 使用。跨 WSL 控制时：

- 模拟器设置 `DAEDALUS_SCENE_CONTROL_BIND=0.0.0.0:5603`；
- WSL 客户端连接 `ip route show default` 返回的 Windows host；
- 客户端只使用 Release SDK 的 `SceneControlClient`，不得手写 UDP JSON；
- 等待模拟器和场景系统就绪，再创建一个会话并在同一会话中发送后续命令；
- 每个命令只允许一层有界重试，整体失败时间应明确且有限。

查看 WSL 侧 host：

```bash
ip route show default | awk '{print $3; exit}'
```

Scene Control 的成功标准是每项都有 `ok` ACK 和应用帧，而不是“界面看起来像切换了”：

1. `create_session`；
2. `set_scene(shooting_range)`；
3. `set_range_target_motion(target=1, ...)`；
4. `set_range_target_motion(target=3, ...)`。

如果 `create_session` 成功而 `set_scene` 超时，先增加有界的启动就绪等待并检查是否存在
另一个 5603 所有者。不要在 shell 和 C++ SDK 两层同时做几十次重试，这会把一次明确
失败延长到数分钟，并可能重复创建会话。

## 4. Windows/WSL 路径损坏

包含中文的 Windows 路径在 Windows PowerShell 5 重定向下可能被错误解码；反斜杠还会
被 Bash 当作转义字符。消费者启动器应显式使用 UTF-8，并把传给 `wslpath` 的路径先
规范化为正斜杠。排障时必须同时打印原始绝对路径和转换结果，转换为空或返回非零时
立即退出，不能继续使用猜测路径。

这属于消费者启动器问题，不是修改模拟器内部路径解析的理由。

## 5. TCP 有监听但没有视觉结果

按以下顺序判断：

1. `capture_processing_complete_total` 是否增长；不增长表示尚未形成离屏图像；
2. `talos_tcp_image_connected` 是否为 1；为 0 表示消费者没有连上；
3. `talos_tcp_image_sent_total/sent_hz` 是否增长；连接后仍不增长才检查发送链路；
4. 消费者输入帧计数是否增长；
5. TensorRT/PnP 的完整视觉计数是否增长。

首次运行可能需要编译消费者桥接器或加载 TensorRT engine。测试计时必须从消费者真正
进入运行态后开始；构建过程耗尽一个 30/60 秒 wrapper 的运行时间不能记为“模拟器 0 FPS”。

跨 Windows/WSL 不得退回文件图像模式规避 TCP 问题。文件三缓冲在 1440×1080 下会因
读取期间槽位更新而大量拒帧，正式数据面固定为 TCP latest-only。

## 6. TensorRT 没有自动启动

这是设计边界，不是故障。模拟器负责渲染、物理、真值、SDK 和数据传输；TensorRT 属于
自瞄或打符消费者。消费者启动器必须显式配置 TensorRT、模型路径和后端，并从运行日志
验证实际后端，不能只根据 CMake 选项推断已经启用。

模型、ONNX、TensorRT engine、checkpoint 和训练数据是受保护资产，不得在排障清理中
删除、覆盖或重新生成。

## 7. 性能异常的正确判断方法

不要混用以下指标：

- `main_update_hz`：模拟器主更新频率；
- `capture_copy_submit_hz`：离屏图像 GPU copy 提交率；
- `talos_tcp_image_sent_hz`：连接消费者后的实际 TCP 图像发送率；
- `preview_present_hz`：可见预览刷新率，上限约 60 Hz；
- 消费者完整视觉频率：检测、PnP 等完整流水线结果率。

性能复现必须使用正式 Release、固定 `1440×1080`、记录后台负载和 GPU 状态，并同时报告
丢帧、GPU map 错误、TCP 错误和曝光匹配。配置上限 200 Hz 不是承诺帧率，单看预览 60 Hz
也不能说明采集只有 60 Hz。完整基线见 `SIMULATOR_PERFORMANCE.md`。

## 安全恢复后的验收清单

一次修复只有同时满足以下条件才算完成：

1. 模拟器正式 Release 能独立启动，版本和配置明确；
2. 高性能模式采集完成计数增长；可视验收模式使用 Vulkan 且画面正常；
3. 需要动态靶场时，四个 Scene Control 操作均返回 `ok` ACK；
4. 连接消费者后，TCP connected/sent 和消费者输入计数均大于 0；
5. 需要 TensorRT 时，日志证明实际后端已启用且完整视觉计数大于 0；
6. stats 中没有 `tcp_bind_failed`，采集丢帧和 GPU map 错误符合测试门限；
7. 退出后没有本次创建的 `daedalus.exe`、Linux bridge 或 5602/5603 占用；
8. 立即重复相同命令，第二次仍能正常启动和退出。

## 疑似模拟器缺陷/需求的审批提案模板

消费者任务不得直接修改模拟器。向用户提交以下信息并等待明确批准：

```text
标题：<一句话症状>
消费者：<aim-stack 提交、模块、启动命令>
模拟器：<Release 版本、source_commit、SDK/SHM 版本>
模式：<高性能 DX12 / 可视 Vulkan>，1440×1080
最小复现：<只含必要步骤的命令>
期望行为：<契约或文档依据>
实际行为：<退出码、错误文本、发生时间>
隔离结果：<纯模拟器 / SDK 客户端 / 完整消费者分别是否复现>
证据：<stats JSON、端口所有者、进程树、WSL ps、相关日志>
影响：<阻塞的模块和阶段>
建议公共变化：<SDK/协议/Release 是否需要变更；不确定则写待评估>
```

用户批准后，修改仍必须在 `daedalus-simulator/main` 独立完成、测试并发布新版本；消费者
只更新 Release/SDK 锁，不复制模拟器实现。
