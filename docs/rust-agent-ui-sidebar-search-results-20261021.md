# Sidebar 搜索结果成员投影与原库存恢复

此前 demo 只向 Sidebar 发布 `SidebarSearchInput` 页脚，完整 `SidebarSearchPublication.hits` 没有进入行列表，因此有搜索结果时仍展示全部库存。本轮接通原 reader 的完整 publication；查询、取消、重试和连接代数仍由原 presentation controller 拥有，没有另写查询状态机或搜索索引。

## 当前 ClientKit 对照

当前 `ClientAgentSidebarModel.rows` 返回 `sessionRows`，其来源为 `ClientMirrorStore.sessionSidebarSnapshot` 的同一SQL读取：本地FTS身份与host搜索身份做OR，已有标题/活动排序、文件夹及置顶规则随后生成列表。结果不经过第二次标题匹配。`ClientAgentSidebarListView.observe` 将当前query的host snippet交给 `outline.rowHelp`，不是增加一个可见摘要行；Tekes的 `SidebarColumnController` 只拥有外层搜索/导航控件。

因此没有照搬当前主路径未使用的旧 `rowSpecs` 搜索分组/header/count。完整本地FTS OR/BM25、排序偏好、文件夹outline和跨端聚合仍待补齐。本轮完成**公开provider结果到当前单端库存**的投影，不把它宣称为完整FTS或Sidebar对齐。

## 公开接入

`SidebarPublication::project_search(endpoint, publication, selected, display_revision)` 返回 `SidebarSearchProjection`：

- 用精确 `SessionAddress` 联结当前库存与原命中，保留库存输入顺序；正文命中的标题可以完全不包含query，不按标题二次过滤，也不按provider顺序改写宿主排序。
- 未知ID不制造会话、不改导航或草稿；当前库存尚未完整联结时，`loaded=false`，不确认空。原公开inventory恢复后，同一query即可联结，无需重新搜索。
- 原库存error与selected保持；清空query恢复完整原库存，清空snippet；畸形、重复、跨端命中和残留clear payload被拒绝。
- `snippets` 是当前query的帮助元数据；原provider的扩展信息仍在原publication，不由UI重新解释。

实际宿主使用 `bind_sidebar_search_publication` / `update_sidebar_search_publication`，先验证输入，再复用原owner/epoch/generation/revision门禁；同revision而hits/snippet不同的冲突被拒绝。`display_revision` 是宿主已有显示发布计数，不授予IO或导航权限。独立consumer仅用两个公开UI包消费投影与空状态。

Sidebar继续使用原List/Scroll/Button/Text控件和库样式。空/待就绪文案通过 `SidebarSearchLabels.no_results/pending_results` 由宿主覆盖；状态变化会刷新原空状态。过滤掉的行会退休其原动作ID，清空后重新出现也不会接收旧点击；持续可见的行保留原控件。没有自建focus/hit/hover/渲染逻辑。

固定SDK已有 `ContentSemantics.description` 及平台AX help映射；当前原Content仍缺动态行帮助声明与统一Tooltip路由，已提交 [TekesUI #35](https://github.com/TekesApps/TekesUI/issues/35)。SDK负责人已接手，后续需明确固定输入后消费。现在不把snippet另画成字幕，也不在AgentKit复制Tooltip状态机或屏幕定位。

## 实际验证边界

原生fixture经原公开RPC失败→用户点击Retry→未知命中触发原公开inventory恢复→只显示3条命中→清空恢复7条库存。原query回调只一次、旧Retry被拒绝、持续可见行保留原Button owner、当前Run草稿保持且无提交。已查看两张实际窗口截图，确认3条与7条的区别。

第一次截图命名为results，但异步捕获落在清空之后；该图保留为未接受的视觉证据。新验收先等待新路径的截图实际落盘，再清空；对应v2日志、二进制和截图独立保留。没有用模型断言代替像素验收。

另复用实际 Kernel0.9.1、Pi1.3.0、DSH0.2.1-rc.2与隔离本地provider，经普通会话历史写入唯一query，再走公开search和inventory恢复。三端均确认返回的命中进入列表、库存顺序与query隔离snippet保持；这部分是离屏协议/GPU证据，`nativeInteraction=false`，不充作三端物理输入法或原生分页/阅读锚点验收。DSH只在自己的临时测试部署开启内存索引，已安装部署不改动。Kernel只读取固定配置blob并使用已有0.9.1二进制，没有改主checkout源码。

默认公开TekesUI pin仍为8cf0840，AgentKit已发布headless契约为v0.3.8；a30b18候选与之前输入保持不变。默认、固定候选、第二consumer、三端和原生窗口的终态、源哈希及缓存清理见同名JSON。完整Views、物理IME、通用行帮助、原生三端阅读/分页和最终review/CI/合并/tag仍未全部完成。

最终验证：默认公开pin全workspace **356** 项（独立consumer **36** 项），固定a30候选全workspace **379** 项（22项专项与原wrapper委托检查）；两者all-target严格Clippy通过。fmt、Rust UI/无UI边界与8包/199文件公开pin核验通过。

本阶段复用既有target；全部本轮编译结束后，以目标目录排他Cargo锁、当前metadata/依赖路径和lsof检查，只删除4个已被替代实验快照对应的206个中间deps文件，释放369,590,272分配字节（约352MiB）。所有源码、固定SDK快照、验收二进制、日志、收据和截图保留。
