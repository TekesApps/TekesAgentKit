# Sidebar 完整文件夹目录输入

对照 ClientKit `ClientMirrorStore.sessionSidebarSnapshot`：文件夹选项在搜索、文件夹筛选和 LIMIT 之前，来自全部 active 普通/置顶会话。按文件夹的最大有效活动时间降序排列，同时间按文件夹 key；路径采用组内 MAX(path)。Tekes 的 `ClientAgentSidebarHostAdapter` 继续只提供宿主能力，目录模型由 AgentKit 持有。

## 公开接入

`SidebarPublication` 新增 `activity_at: BTreeMap<SessionAddress, f64>`。`SidebarBinding` 在同一次成功库存转换中发布 rows 和其有效活动时间，复用既有运行期间冻结规则；不会为了目录重新请求 provider、打开历史或读取私有文件。失败保留 rows、时钟及原 revision 递增规则，旧会话时钟随库存替换退休。

调用 `publication.folder_catalog()?` 获取 `SidebarFolderCatalog { revision, loaded, error, folders }`，每个选项为 `SidebarFolderOption { key, path }`。目录不读取显示页、搜索投影、已选行 supplement、标题排序或折叠状态。未恢复时保持 loaded=false 且无可选目录；恢复后的观察错误保留最近目录并带 error。纯函数 `sidebar_folder_catalog(rows, activity_at)` 可供其他规范库存发布者复用。

手工构造 publication 的消费者必须为所有行提供规范有限时钟；缺失、NaN/infinity、空会话 ID、非法文件夹 ID 与重复会话拒绝，不猜测时间或以页内行反推目录。旧夹具只构造列表、尚不消费目录时可先提供空 map；正式宿主消费目录时不可用空 map 掩盖未知时间。

```rust
let catalog = publication.folder_catalog()?;
if catalog.loaded {
    // Render via SDK controls when the standard menu/event API is available.
    for option in &catalog.folders {
        // Keep option.key typed; pass it to set_sidebar_folder_filter.
        // Product copy for missing/empty path belongs to the host.
    }
}
```

`ComponentDemo::sidebar_folder_catalog()` 从完整绑定 source 返回同一公开类型，没有第二套目录状态。时钟单独变化也随 source revision 更新目录，不需要伪造列表行变化。第二公开 consumer 的 `sidebar_folder_choices` 直接消费相同接口。

## 身份与布局

成员身份沿用 endpoint + Id/Path/None。显式 ID 优先；路径只是标签，绝不当作会话路由。排序比较采用 ClientKit 的 key 文本顺序，同时以 typed key 作最终 tie-break，避免 Id("path:x") 和 Path("x") 因拼接碰撞丢失一个真实目录。不同端点始终隔离。

目录包含页外会话和仅有 pin 的目录，但分组标题仍仅来自当前实际显示的普通行。它不是空 Primary Folder 的管理清单，不制造无会话目录；按文件夹新建、管理路径映射、跨端聚合及跨进程活动时钟仍未完成。当前 binding 只有公开 cwd 时提供 Path 身份，不虚构 primary folder ID。

标准选项菜单仍待 TekesUI #36，未复制菜单或更改 SDK；#22/#28 的公开 reading/reveal 缺口和默认8cf精确依赖不变。此接口补齐宿主可接的状态输入，不等于完整 Sidebar 视觉验收或自动 reveal 交付。

## 验证

新测试覆盖完整目录与显示分页/筛选/标题顺序的分离、pin-only 及页外目录、MAX(path)/MAX(activity)、同时间稳定排序、typed key 碰撞、未知时钟/重复/非法源拒绝、恢复/失败保留，以及真实 ComponentDemo 和第二公开 API consumer。原库存观察测试同时验证运行期间有效时钟冻结，不用 summary 的流式新时间代替。

本轮精确执行结果见同名 JSON；组件与构建证据和原生窗口/物理 IME/VoiceOver 证据分开，未执行的原生项目不得计为通过。

最终默认UI401项、第二consumer42项及严格Clippy/实际依赖清单/静态边界/native build通过。首次定向断言错选词典排序后仍在页内的行，修正后通过；首次Clippy发现无用import，删除后完整复验通过，原失败日志保留。未执行本提交的原生窗口、物理IME或私有候选回放；核心实现未变，未重跑核心套件。所有验证复用现有共享target，未新建target，也未删除用途不明确的现用缓存，本阶段实际释放0字节。
