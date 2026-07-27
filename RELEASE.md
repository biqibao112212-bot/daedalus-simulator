# Daedalus 正式发布流程

版本唯一来源是 `VERSION`，发布契约是 `release/release.json`，SDK 契约是 `sdk/contract.json`。三者必须一致。

## 构建与测试

```powershell
Set-Location D:\仿真\repos\daedalus-simulator
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\check-compatibility.ps1
```

该流程构建 Rust Release，并在 WSL 中构建、测试 C++ SDK。

## 生成发布包

工作树必须干净并已提交：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\package-release.ps1
```

默认输出：

```text
D:\仿真\releases\daedalus-simulator\1.0.3\
D:\仿真\releases\daedalus-simulator\1.0.3.zip
```

发布目录包含：

- `bin/daedalus.exe` 及运行 DLL；
- `assets/`、两套固定配置和启动脚本；
- `sdk/` 下可被 `find_package(DaedalusSimSdk 1 CONFIG REQUIRED)` 发现的完整开发包；
- 接口、性能、故障排查和契约文档；
- `release-manifest.json` 文件大小与 SHA256 清单。

发布提交通过 clean 状态复验后创建注释标签 `simulator-v<version>`。只有用户明确授权时才推送提交、标签和 ZIP。
