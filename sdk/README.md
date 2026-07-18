# Daedalus 模拟器 SDK 1.0

SDK 是模拟器对自瞄和打符开放的唯一源码接口。消费者不得复制 Talos
共享内存结构、图像尺寸或 ABI 常量，也不得直接依赖模拟器内部 Rust 模块。

固定契约：

- SDK：`1.0.0`
- Talos SHM：`v7`
- 图像：RGB `1440×1080`
- 元数据区域：`76992` 字节
- 消费者：自瞄与打符共用同一个 `DaedalusSimSdk::DaedalusSimSdk`
- ABI：`talos_v1.hpp`
- TCP 图像协议：`tcp_image_v1.hpp`
- 端点与默认端口：`endpoints_v1.hpp`

构建、测试和安装：

```bash
cmake -S sdk/cpp -B build/sim-sdk -DCMAKE_BUILD_TYPE=Release
cmake --build build/sim-sdk --parallel
ctest --test-dir build/sim-sdk --output-on-failure
cmake --install build/sim-sdk --prefix build/sim-sdk-install
```

消费者配置：

```bash
cmake -S <consumer> -B <build> \
  -DCMAKE_PREFIX_PATH=<simulator>/build/sim-sdk-install
```

消费者按需包含：

```cpp
#include <daedalus_sim_sdk/talos_v1.hpp>
#include <daedalus_sim_sdk/tcp_image_v1.hpp>
#include <daedalus_sim_sdk/endpoints_v1.hpp>
```

接口变更由模拟器仓库独立发布。消费者只更新 SDK 版本并运行契约测试，
不再维护分支私有 IPC 布局。
