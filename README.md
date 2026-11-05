# EmbyIcons Rust

为 Emby 的 Primary 海报制作评分、分级和收藏叠加图。当前版本 `5.61.1.7-nevermore.rust.1`，目标环境为 Linux x86_64、Emby 4.10.2.0。

## 分支

- [`main`](https://github.com/NevermoreN/EmbyIcons/tree/main)：Rust 业务实现，保留 Emby 必需的薄 .NET 接口适配层及 HTML/JavaScript 设置页面。
- [`legacy-csharp`](https://github.com/NevermoreN/EmbyIcons/tree/legacy-csharp)：重构前的完整 C# 版本，基线提交 `000ee767e29b7a39027689b15796d3ea52166463`。

## 功能

- 入库后自动排入后台队列，原图和元数据可用后主动制图；元数据、收藏等相关变化自动重新核验。
- 手动计划任务 `EmbyIconsPreDrawPosters` 使用同一队列，报告制作、复用、等待和失败数量。
- 普通浏览只读取已发布的成品，不触发绘图或补图排队；后台维护独立补齐缺失成品。
- 社区评分、烂番茄、爆米花、MAL、分级、收藏数六类叠加，保留现用两套布局。
- 原图只读，保留原尺寸，使用无损 PNG；沿用 Emby 的增强缓存和缩放缓存，复用已有有效成品。
- 保留配置、预览、导入导出、图标统计和海报排障。

当前范围不包含季/单集、音乐、技术参数聚合、自定义图标目录、文件名/标签规则或海报缩放归一化，详见 [范围说明](docs/05-current-usage-and-scope.md)。旧配置中的兼容字段保留用于无损读取和保存，不代表恢复已移除的业务。

## 架构

| 路径 | 职责 |
| --- | --- |
| `native/embyicons-core/` | Rust 核心：规则、标识、队列、状态、评分、绘图及管理业务 |
| `Host/` | Emby 生命周期、事件、图片处理器、计划任务及 API 的机械适配 |
| `Interop/` | Rust ABI、句柄与缓冲区管理 |
| `Configuration/` | 兼容配置 DTO 和设置页面资源 |
| `tools/` | 构建、打包及保留的 Rust 宿主缓存契约验证工具 |

主分支已删除旧 C# 业务、旧后端测试、参考后端构建模式及旧绘图对比工具。Rust 的 `legacy.rs` 用于读取既有磁盘缓存，不是旧 C# 后端。

## 构建

需要 Linux x86_64、Rust/Cargo、C/C++ 链接器、Python 3、dpkg-deb 和 .NET 8 SDK：

```sh
tools/build-linux-x64.sh /path/to/release
```

脚本下载并校验固定依赖，只在临时目录解包和构建，不安装系统包。输出单个 `embyicons.dll`，其中内嵌 Rust 核心、私有运行库和许可证。细节见 [原生构建说明](native/embyicons-core/README.md)。

## 安装与缓存

备份当前 DLL、`plugins/configurations/embyicons.xml`、`data/EmbyIcons` 和 `plugins/embyicons`。停止 Emby，将新包安装为 **`plugins/embyicons.dll`**，然后启动；保持小写文件名，避免加载重复副本。

插件在启动时自动解压到 `plugins/embyicons/native/<包 SHA-256>/`，校验后复用。不会改动 Emby 自带库、系统库或全局库搜索路径。

成品保存在 Emby 的 `cache/images/enhanced-images`，缩放结果保存在 `cache/images/resized-images`；新状态保存在 `data/EmbyIcons/rust-state-v1`。旧 `image-state-v2` 保留原位用于迁移和回退。升级不清空图片缓存，不修改 `PersistedVersion`，不覆盖原图。

插件名称为 **EmbyIcons (NevermoreN)**，独立 ID 为 `5ba28f85-65d0-4c24-b4b2-4f195826c549`，与官方目录条目区分。本分支不会自动下载 GitHub 构建。

## 文档与验证

[架构与计划](docs/README.md) · [生产实施与验收](docs/07-implementation-status.md) · [Rust 主分支整理](docs/08-rust-only-main.md) · [验证工具](tools/README.md)

只运行改动所需的最少验证；不要求迁移旧后端全部测试，也不反复执行同义测试或性能基准。历史性能和生产证据保留在实施记录中。

## 许可证

遵循仓库 [LICENSE](LICENSE)。私有运行库的许可证随原生包提供；旧版本及其来源记录保留在 Git 历史和旧分支中。
