# Daedalus Simulator platform support

This release targets the x86 family as `x86_64` only. It supports:

| Package | Rust target | Binary | Performance backend | Visible backend |
| --- | --- | --- | --- | --- |
| Windows x86_64 | `x86_64-pc-windows-msvc` | `bin/daedalus.exe` | DX12 | Vulkan |
| Linux x86_64 | `x86_64-unknown-linux-gnu` | `bin/daedalus` | Vulkan | Vulkan |

32-bit `i686` and ARM targets are not release targets. This is intentional:
the simulator, wgpu drivers, and the fixed SDK ABI are validated as 64-bit
artifacts.

## Build and package

Build the Windows package from a Windows host with Rust, CMake, CTest, and a
Visual C++ toolchain installed:

```powershell
.\scripts\build-release.ps1 -Platform windows -Arch x86_64
.\scripts\check-compatibility.ps1
.\scripts\package-release.ps1 -Platform windows -Arch x86_64
```

Build the Linux package on an x86_64 Linux host or Linux CI runner:

```bash
./scripts/build-release.sh
./scripts/package-release.sh
```

The SDK is built, tested, and installed separately for each target. A Windows
package must never reuse the Linux SDK install tree. The release directory is
organized as `<version>/<platform>-<arch>` so the two packages can coexist.

## GPU rendering versus inference

The simulator package does not include GPU drivers, CUDA, cuDNN, TensorRT,
ONNX files, or TensorRT engines. Rendering uses the system GPU driver through
wgpu and is selected by the launcher with `WGPU_BACKEND`.

CUDA/TensorRT inference is a consumer-owned runtime. The consumer bridge is
built against its selected CUDA/TensorRT profile and connects to the simulator
through the versioned SDK, TCP image stream, and UDP command protocols. This
allows multiple inference profiles without rebuilding or mixing inference DLLs
into the simulator package.

The only existing end-to-end inference baseline recorded by this repository is
Windows simulator + WSL consumer bridge with CUDA 12.8 and TensorRT 10.9.0.34.
Other CUDA/TensorRT combinations are possible only after the consumer bridge
is rebuilt and the joint test proves the actual backend, engine hash, frame
counts, dropped frames, GPU-map errors, and exposure matching.

## Runtime requirements and validation gates

Install a current graphics driver with the selected backend. Do not copy GPU
drivers into the release directory. The default TCP contract is RGBA32
1440x1080 (legacy SHM slots remain RGB24),
SHM v7 / ABI revision 2, TCP image port 5602, UDP command port 5601, and scene
control port 5603.

The internal-lab Linux acceptance gate intentionally does not require a
discrete GPU. A software Vulkan implementation such as Mesa llvmpipe is enough
to validate the native x86_64 binary, package startup, endpoints, IPC creation,
and runtime capability reporting. This is a compatibility/diagnostic mode, not
a real-time performance promise: fixed 1440x1080 RGB delivery and a complete
live auto-aim loop require a suitable hardware Vulkan adapter. Seeing a GPU in
`nvidia-smi` alone does not prove that the Vulkan loader can use it.

Release binaries embed their simulation configuration and disable config hot
reload, local keyboard/mouse mutation, debug/auto-generation modes, dataset
output, ground-truth publication, and managed inference bridges. Only visible
versus performance presentation mode and the IPC directory remain deployment
choices. Camera calibration is fixed and read-only; no detection/PnP result
upload API exists.

Before publishing a package, verify:

1. The binary matches the target triple and the package contains no binary for
   the other operating system.
2. The target-platform SDK CMake build and all CTest tests pass.
3. `release-manifest.json` contains the source commit and SHA256 for every
   package file.
4. The launcher works from a directory other than the package directory.
5. The package contains no CUDA, TensorRT, ONNX, engine, or model assets.
6. A consumer-side joint test proves the selected inference profile when one
   is required; simulator-only tests must not be presented as inference tests.
