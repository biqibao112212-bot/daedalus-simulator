# Daedalus Simulator release decisions

1. Work is isolated in `D:\仿真\isolated\daedalus-simulator-multiplatform-x86`
   on branch `release/simulator-multiplatform-x86`; the canonical `main`
   checkout is never switched or written by this task.
2. The release architecture uses two native build paths: PowerShell for
   Windows MSVC and Bash for Linux GNU. A Windows WSL SDK build must not be
   reused as a Windows SDK artifact.
3. The release architecture targets x86_64 only. 32-bit i686 and ARM are not
   promised because the fixed SDK layout and GPU/runtime validation are 64-bit.
4. Release directories are version/platform/architecture scoped so Windows
   and Linux packages can coexist without replacing protected artifacts.
5. Windows defaults to DX12 for high-performance mode and Vulkan for visible
   validation. Linux defaults to Vulkan for both modes. GPU drivers remain
   system dependencies.
6. The simulator does not bundle or link CUDA, cuDNN, TensorRT, ONNX, or engine
   files. Inference versions are consumer-owned build profiles; the simulator
   package is unchanged between profiles. Joint tests must prove the actual
   consumer backend and engine hash.
7. The release candidate boundary is SDK 1.1.0, SHM v7, ABI revision 2,
   default TCP RGBA32 1440x1080 (legacy SHM RGB24), TCP 5602, UDP 5601, and
   scene control 5603. ABI revision 2
   adds frame/command identity and typed actual gimbal feedback while keeping
   the metadata region size unchanged.
8. A formal package requires a clean committed source revision, target-native
   Rust/CMake tests, a platform-specific binary/SDK pair, and a SHA256 file
   manifest. Dirty local builds are evidence for development only.
9. The Linux packager uses native Git on native Linux and falls back to
   `git.exe` only when WSL is reading a Windows-created worktree whose `.git`
   pointer contains a Windows path. This keeps local WSL packaging diagnostic
   without weakening the clean-tree gate.
10. Distribution builds compile with `distribution-release`: configuration is
    embedded, hot reload and local mutation/debug paths are disabled, network
    controls bind to loopback, and ground truth is not published.
11. Fixed camera calibration is read-only. Revision 2 records the digital
    camera geometry and fixes renderer exposure at EV100 9.7 with auto exposure
    and tonemapping disabled. Physical shutter time and analog gain do not
    apply to this digital renderer. No calibration setter or vision-result
    upload API is added.
12. wgpu owns adapter selection; the SDK reads the actual selected adapter and
    driver from the runtime capabilities record. CUDA/TensorRT remains wholly
    consumer-owned and absent from the package.
13. A read-only Agent Team audit identified same-user IPC bypass limitations
    and the repository AGPL-3.0 license. Strong anti-bypass requires OS account
    or container isolation. The active package profile is now non-commercial
    use inside the owning laboratory: it carries the repository license and an
    internal-use notice without requiring a commercial license file. External
    or public distribution remains outside this decision and requires review.
14. A complete auto-aim loop must not pair an image with an arbitrary latest
    gimbal state. Distribution builds publish an empty-target exposure record
    into the existing 16-slot history, and SDK consumers query exact exposure
    state by the TCP image `source_sequence`.
15. The Chinese user guide explains the end-to-end workflow; a separate
    `docs/SDK_API_REFERENCE_ZH.md` is the function-level source of truth for
    signatures, parameters, return values, errors, and usage examples.
16. Windows release acceptance uses Visual Studio multi-config explicitly in
    Release mode and enables `/FS` for parallel MSVC PDB writes. Headless
    performance mode must use `ExitCondition::DontExit`; otherwise the absence
    of a primary window terminates a valid release process.
17. Windows is the full runtime acceptance platform: DX12 validates the image,
    timestamp, synchronized gimbal, scene-control, and command-feedback loop;
    Vulkan validates the visible rendered window on the selected discrete GPU.
18. The internal-lab Linux gate intentionally accepts native x86_64 build/SDK
    tests plus Mesa llvmpipe software-Vulkan startup, ports, IPC, and runtime
    capability reporting. It does not require a discrete GPU and makes no
    real-time 1440x1080 RGB or complete live auto-aim performance claim.
