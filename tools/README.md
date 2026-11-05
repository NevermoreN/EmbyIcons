# 最小宿主验证工具

宿主工具用于本次重构的缺口验证，不是常驻服务。证据及限制见 [实施记录](../docs/07-implementation-status.md)。不需要重复运行已经通过且未受改动影响的场景。

## HostCacheProbe

链接正式 `Interop/NativeCore.cs`，调用实际宿主 `ImageProcessor` 与 `ImageService`，编解码器和结果工厂使用替身。验证发布/回退/恢复路径，不能用它判断真实图片质量或绘图性能。

需要 .NET 8 SDK、Rust 原生库，以及自己当前 Emby 的 `Emby.Drawing`、`Emby.Api`、`Emby.Server.Implementations`、`MediaBrowser.Controller`、`MediaBrowser.Model`、`MediaBrowser.Common`、`Emby.Media.Model` DLL。宿主 DLL 只存放私有临时目录，不放进 Git。

从仓库根目录执行，其中 `scratch` 为专用临时目录，`host_path` 为对应宿主 DLL 目录：

```sh
CARGO_TARGET_DIR="$scratch/native" cargo build --release --locked --manifest-path native/embyicons-core/Cargo.toml
dotnet build tools/HostCacheProbe -c Release -p:EmbyHostPath="$host_path" --artifacts-path "$scratch/build"
LD_LIBRARY_PATH="$scratch/native/release${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" dotnet "$scratch/build/bin/HostCacheProbe/release/HostCacheProbe.dll"
```

工具自己的合成文件在 finally 中删除；专用构建目录由调用者清理。旧 C# 对照模式、RendererComparison 和一次性 HostEventProbe 已移除；历史证据见实施记录。

## 打包与依赖

`build-linux-x64.sh` 组合固定依赖下载、Rust release、原生 ZIP 及 .NET 单 DLL 构建。`fetch-native-dependencies.py` 按公开摘要下载解包、不安装系统包；`package-native-linux.py` 校验运行库 glibc 要求并打包许可证。

生产 DLL 启动时自行解压，不需要安装探针或单独复制 `.so`。详见 [Rust 核心构建说明](../native/embyicons-core/README.md)。Rust 主分支整理时已执行一次完整组合构建，确认删除旧源码后仍可生成单 DLL；没有重复绘图基准或生产生命周期测试。

HostCacheProbe 的 `--legacy-fixtures /private/fixtures.json` 模式只把已重建的旧键交给当前宿主计算 Tag，与私有生产记录对照；不运行图片绘制。夹具包含 original、ticks、kind、id、key、expectedTag，不能提交真实媒体夹具。
