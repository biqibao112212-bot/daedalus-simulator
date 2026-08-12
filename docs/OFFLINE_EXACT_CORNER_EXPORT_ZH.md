# 离线同曝光 exact-corner 标签导出

状态：Daedalus Simulator 1.3.0 公共研究合同。

## 边界

导出器是只写离线旁路，默认关闭。只有
`DAEDALUS_CORNER_LABELS_JSONL` 指向模拟器可执行文件/Release 目录之外、父目录已存在、
尚不存在的绝对 `.jsonl` 路径时才启用；模拟器绝不覆盖既有标签。

该开关不会解锁实时真值。distribution 构建通过 SDK 发布的 `target_count` 与
`rune_count` 仍固定为 0；TCP 图像协议、SHM v7 / ABI revision 2 以及
5601/5602/5603 端口均不改变。标签不会进入 detector、PnP、预测器或云台控制输入。

## 严格同曝光身份

每行对应真实 TCP 图像中的一块小装甲板，唯一联结键为
`(producer_epoch, frame_seq, timestamp_ns)`。角点、相机和运动状态与 GPU 图像在同一次
Bevy `ExtractSchedule` 中冻结；私有 sidecar 随图像经过 GPU readback 和 latest-only TCP
mailbox，只有 TCP header 与 RGBA payload 完整写入活动连接后才追加 JSONL。

因此无客户端、submit 拒绝、mailbox 替换、GPU 采集失败、部分 socket 写入、断连或身份
不一致都不会写标签。标签身份集合只能是真实已发送图像身份集合的子集，禁止用 Scene
Control 帧号或最新状态替代三元曝光身份。

## 真实几何与角点顺序

schema v1 只导出 Shooting Range 当前活动的 #3 靶车小装甲板。公共 nominal 规格为
135×55 mm，但 exact pixels 使用 `assets/vehicle.glb` 内真实 `*_ARMOR_MARKER` 四顶点；
资产 SHA256 为
`1cc0a3cd1ab05bc9822b616271db3afb64d078e56b9bbf452a8acc6d9bad0a6f`。
真实四边形约 133.77×53.89 mm，存在约 4–5 μm 的非严格矩形/非共面误差，倾角约 15°。
每行同时写 nominal 尺寸和真实 `object_corners_armor_m`；free-IPPE 的 exact 闭合必须使用
后者，不能为了得到 135×55 而构造理想矩形。
marker 保留资产原始约 4–5 μm 的非严格共面形状，因此掠视或出画视角中的通用平面 OpenCV
IPPE 是独立闭合检查，而非代数逆。随包 validator 默认 RMS/等效米制上限为
`0.025 px` / `0.125 mm`；只有对具有证据支持的受限采集几何，才应通过
`--max-reprojection-px` 或 `--max-equivalent-error-m` 收紧阈值。

Shooting Range #1 使用 `HERO.glb`，其大装甲约 228.77×53.89 mm，明确不属于 schema v1，
不会被伪标为 135×55。

`exact_corners_px` 与 `object_corners_armor_m` 使用相同的屏幕规范顺序
`bl,tl,tr,br`，屏幕 x 向右、y 向下。像素保留亚像素精度且允许落在画面外；任一角在相机
后方、非有限或四边形退化时 fail closed，不写该行。

`visibility` 只给出保守的场景层级/画面范围语义：是否 scene-hidden、四角有几个落在画面
内。高性能 TCP 路径没有 depth occlusion 验证，因此固定
`occlusion_tested=false`，不得把它解读为“无遮挡可见”。

## 运动字段

线速度和角速度来自同曝光时刻靶车运动学刚体的实际规定轨迹导数，并转换为 ROS odom
世界系，单位分别为 m/s 与 rad/s。`distance_m` 是曝光相机光心到真实 marker 中心的距离。

`motion_uniform` 使用固定 100 ms 保护区。目标首次出现或运动状态变化后 100 ms 内为
false；往返平移只有在当前位置距最近确定性端点严格大于
`speed * 0.100 s + 0.001 m` 时才为 true，因此端点、反向瞬间及其两侧确定性邻域都会被
排除。稳定静止和恒定自转段可以为 true。实现不输出未来位姿、未来命令，不用运动指令
近似角点，也不建立端点预测模型；每行固定 `future_truth_included=false`。

## 使用与验证

先创建独立运行目录，再选择一个新文件：

```powershell
$session = 'D:\仿真\runtime\corner-label-session-001'
New-Item -ItemType Directory -Path $session
Set-Location D:\仿真\releases\daedalus-simulator\1.3.0\windows-x86_64
.\start-simulator.ps1 -CornerLabelsJsonl (Join-Path $session 'exact-corners.jsonl')
```

在第二个终端运行包内采集器。它会创建 Scene Control v2 会话、切到 Shooting Range #3、
设置短行程往返运动、完整读取真实 TCP 帧，并保存每帧 wire 身份和 RGBA payload hash：

```powershell
D:\Anaconda\envs\yolov8\python.exe .\docs\capture-corner-label-experiment.py `
  --output-dir D:\仿真\runtime\corner-label-session-001 `
  --until-eof --linear-span-m 0.6 --save-first-rgba
```

至少运行数个往返周期后停止模拟器；采集器会把 TCP 连接上已存在的完整帧全部排空到
EOF，随后才关闭身份台账。严格的“已接收帧覆盖全部标签身份”验收必须这样协调停止；
固定帧数客户端会有意遗漏其退出后已经进入 socket 缓冲的帧。

没有活动 TCP 客户端时标签文件保持为空。采集完成后，用带 OpenCV 的 Python 验证
schema、同曝光身份、Z4 完整性、运动排除和 free-IPPE 闭合：

```powershell
D:\Anaconda\envs\yolov8\python.exe .\docs\verify-corner-label-export.py `
  D:\仿真\runtime\corner-label-session-001\exact-corners.jsonl `
  --tcp-identities D:\仿真\runtime\corner-label-session-001\tcp-identities.jsonl `
  --require-complete-z4 --require-uniform-and-excluded
```

身份文件每行保存一个真实接收的 TCP 三元身份和 payload hash。验证器检查包内 schema、
必填字段、资产 hash、唯一性、无未来真值，并用 OpenCV generic `SOLVEPNP_IPPE` 检查
像素重投影与等效米制闭合。JSONL、身份台账、原始帧与实验目录均为受保护采集资产，
不得进入模拟器 Release 包。

机器可读 schema：`schemas/offline-exact-corners-v1.schema.json`。
