# EmbyIcons Rust 核心

当前实现：Rust 负责海报业务；Emby 的 .NET 适配层负责宿主接口和调用。实际验收及上线状态见 [07 实施记录](../../docs/07-implementation-status.md)。目标为 Linux x86_64、Emby 4.10.2.0。

## 业务与调用链

`ItemAdded / ItemUpdated / 收藏变化 / 合集变化 → .NET 标量 DTO → Rust 去重队列 → 输入核验与评分 → 冻结作业 → Emby GetEnhancedImage → Rust 全尺寸绘图与无损 PNG → 宿主返回路径核验 → Rust 发布`。

手动计划任务也使用该队列，报告制图、复用、等待和失败数。普通图片回调只查询已发布状态，不绘图、不查询评分、不排修复队列；后台维护独立修复缺失成品。

- `engine.rs`：Profile/库映射、稳定画面指纹、有限重试、过期作业拒绝、分区持久化、手动任务统计。
- `legacy.rs`：读取旧 `image-state-v2` 及旧绑定、别名和恢复标记，探测已有宿主成品；缺失才转为新 PNG 标识。旧周期间隔仅用于一次性迁移，后续已发布查询不读取时钟。
- `ratings.rs`：共享评分缓存、请求去重、15 秒超时、失败退避及旧评分磁盘格式。评分请求不占绘图槽。
- `render.rs`：六类评分/分级/收藏叠加、两套现用布局、内置图标与字体；原尺寸无损 PNG，不覆盖原图。
- `management.rs`：保留的配置、预览、导入导出、统计与海报排障。

主分支只保留 Rust 业务及 `Host`、`Interop` 等必要适配代码；旧 C# 后端、对应测试和参考构建模式已移至 `legacy-csharp` 分支。

## ABI v2

C 调用约定；`ei_create` 创建上下文，所有在途调用结束后才可 `ei_close`。返回缓冲区由调用方恰好执行一次 `ei_buffer_free`，不使用托管释放器。

| 导出 | 契约 |
| --- | --- |
| `ei_abi_version() -> u32` | 版本 2，不匹配时拒绝加载 |
| `ei_execute(Context*, u8*, size_t, Buffer*) -> i32` | UTF-8 JSON 冷命令；输入上限 1 MiB |
| `ei_query(Context*, u64, QueryResult*) -> i32` | 单次固定结构热查询，无文件、网络和宿主元数据访问 |
| `ei_buffer_free(Buffer)` | 释放原生返回缓冲区 |
| `ei_close(Context*)` | 释放上下文及写入锁 |

`Buffer = { u8* data, size_t length }`，无结尾 NUL 保证。

`QueryResult = { u64 generation, u32 flags, u32 key_length, u8 key[512] }`，共 528 字节。flags 位 0 表示已发布可用，位 1 表示迁移旧 `.jpg` 路径；key 为 UTF-8。未就绪时可能仍有旧 key，不代表可以提供该成品。

状态 0 表示 ABI 调用完成；冷命令另返回 `{"ok":true,"value":...}` 或错误原因。普通请求不可调用 `render`。`stage/begin/hash` 的原型组合仅供缓存契约工具使用，不是生产标识算法。

## 磁盘与原图

成品只写 Emby 提供的 `cache/images/enhanced-images` 目标。Rust 校验规范路径，使用同目录临时文件及原子提交；原图不可用作输出或清理目标。宿主完整调用返回后，还需核验目标文件、Tag、作业版本、原图是否在期间变化，才能发布。

状态位于 `data/EmbyIcons/rust-state-v1`：1024 个校验分区、备份和单写入锁。旧状态只读，保留原位供回退。预览位于独立预览目录，返回流关闭后删除预览成品。

## 构建与自动解压

Emby 进程使用其自带 glibc 2.34，不能按容器操作系统版本选择依赖。禁止替换 Emby 自带库或安装系统依赖；运行库和许可证打包进插件 DLL，启动时解压至：

`<ProgramDataPath>/plugins/embyicons/native/<完整包 SHA-256>/`

相同包校验已解压文件并复用；只写缺失或校验不符的文件。更新包使用新摘要目录。核心 ELF 的私有 RPATH 为 `$ORIGIN/embyicons-native`，不修改环境变量或宿主库目录。

构建机需要 Linux x86_64、Rust/Cargo、C/C++ 链接器、Python 3、dpkg-deb 和 .NET 8 SDK。运行以下命令会下载校验固定依赖、临时解包和链接、打包内嵌 ZIP、编译 DLL，然后清理临时构建目录：

```sh
tools/build-linux-x64.sh /path/to/release
```

Rust 主分支整理时已实际执行一次该组合脚本，完成 Rust、依赖打包和 .NET 单 DLL 构建，零警告、零错误。锁定包版本和摘要见 [依赖清单](../runtime-linux-x64-dependencies.json)。不安装系统依赖，不需要向 Emby 镜像中添加库。

需要拆步或使用现有 Cargo 缓存时，可单独运行 `fetch-native-dependencies.py`、`cargo build --release --locked`、`package-native-linux.py` 和 `dotnet build -p:NativeBundlePath=...`。打包工具的 `--help` 列出具体参数。

只部署最终 `EmbyIcons.dll` 为 `plugins/embyicons.dll`，无需另外安装 `.so`。非目标架构、原生包缺失或 ABI 不符会明确失败，不回退到请求时 C# 绘图。构建临时文件用完清理；保留锁文件、依赖摘要和正式交付包。
