# Sidebar 有界分页与目标行可见性（2026-10-21）

后续选中行独立读取见 [selected-read contract](rust-agent-ui-sidebar-selected-read-20261021.md)。本报告保留原阶段的能力与证据边界。

Rust AgentKit 消费 TekesUI 原控件。参照当前 ClientKit `SidebarPaging.pageSize = 5`、`sessionLimit`、独立 pinned/selected 查询与 `mergingSelectedThread`；Tekes 宿主继续拥有路由、窗口和系统能力。本阶段没有改动这些外部仓库。

## 本地库存分页

Sidebar 从同一权威库存快照计算普通会话页，初始5条；`SidebarLabels.more` 控制原 Button 的“显示更多会话”文案。宿主先 drain 原动作，调用 `take_page_requests`，再调用 `load_more`，每次增加5条。ComponentDemo 已接这条通路，不发 RPC、不更改导航或持久偏好。

置顶行独立于页大小和文件夹筛选。排序、文件夹筛选先于分页，分组由同一个 `sidebar_outline` 实现。搜索已拥有自己的排名和成员集合，不再被普通库存分页截断，也不按当前选择补入无关结果。`paging()` 分别报告 limit、ordinary_count、has_more 和 selected_supplemented。恢复完成前没有 More 控件。

普通已选会话即使在页外，也从当前权威库存补入真实行；不消耗普通页额度，不扩大页大小、不制造重复行，不清除文件夹筛选。它仍使用上一阶段的临时 disclosure 与手动折叠优先规则。选择已离开当前公开库存时继续等待，不能从路由字符串伪造 title/folder/actions。

PageRequest 保留源 Sidebar 生命周期、原控件和原 limit；更新后的旧按钮、重复点击和相同 namespace 的新挂载都不能扩页。展开页不会替换已显示的 row Button。库存刷新保留请求条数；本地分页不会冒充远端 cursor 分页，也不会从 mirror invalidation 自动请求网络。

## 可见性接入

`Sidebar::visibility_target()` / `ComponentDemo::sidebar_visibility_target()` 仅为显式导航提供真实 viewport 和 item ID，普通刷新不生成新的滚动意图。目标须已恢复、已进入分页/筛选成员集合且 disclosure 已打开；手动折叠在偏好确认之前就使排队目标失效。

宿主在实际布局之后保留此 capture，调用 `target.claim(&host)`，或通过独立 consumer 的 `claim_sidebar_visibility` 立即验证，然后将原 item 交给 TekesUI 的保留焦点 reveal。claim 检查原挂载、库存 revision、导航 revision、disclosure generation 和当前活动成员，一次导航最多成功一次。拒绝/失败不自动重试；需要重试时由宿主发出新的显式导航版本。

固定336候选的原 `set_scroll_reading_document` 与 `reveal_scroll_reading_item(Nearest)` 已用于独立消费测试：页外选中行起初完全在视口外，调用后真实可见；原 Composer focus、中文 marked text、选区和编辑 baseline 不变；重复请求被拒绝。没有读 offset 反算坐标、没有手工像素补偿、没有通过 focus 代替 reveal。

当前公开精确 pin8cf 尚无该 focus-preserving item API。此限制沿用 TekesUI #28 / #22，固定336已有实现，不重复提出相同 issue，不提升默认依赖，也不在默认原生 demo 里伪造自动 reveal。原生宿主常驻 reading-document 更新/滚动竞争、物理 IME/VoiceOver 和三端原生分页/目标可见性验收仍需后续联调；本阶段的候选验证为组件/CoreText 布局测试，非原生窗口验收。

## 验证及缓存

默认与固定候选分别验证 UI workspace、第二公开 consumer、严格 all-target Clippy 和 native build；固定候选保留前26项回归并加入1项 Sidebar reveal。所有命令复用 `/tmp/agentkit-layout-text-292-target`，无新独立 target。无 UI 核心源码和三个 SEP 均未改变，本阶段不宣称重跑三端协议验证。可执行文件、日志、固定源和收据保留；精确结果见同名 JSON。

PR #10 仍为草稿。完整选中行独立补读、远端未来的库存 cursor、文件夹管理、多端聚合和完整原生 UI 验收不由本地分页声明完成。

最终默认 workspace386项、固定候选414项、第二公开consumer42项、候选原reading回归27项通过；两边严格all-target Clippy和native build通过。SDK503文件和全部候选AgentKit输入哈希复核未变。终态后持有共享target的单一排他Cargo锁并检查lsof，清理旧navigation receiver与本轮首轮receiver的280个中间文件，`du`差值及去重inode释放量均为213,417,984字节（203.5 MiB）。程序、源码、固定快照及证据保留。
