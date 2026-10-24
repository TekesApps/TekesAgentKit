# Sidebar 标题与最近活动映射

本轮修复真实 inventory 到 Sidebar 的两处差异：原绑定只读 summary 顶层 title/name，Pi 的标题通常在 projection 中；原绑定直接保留按 ID 遍历的库存顺序，没有最近活动排序。默认 TekesUI 仍固定公开 `8cf0840790334ba224afc0595897c74231c712e6`，未更改底层 UI、宿主产品布局或已发布依赖。

## ClientKit 与宿主对照

只读对照当前 `ClientSessionSidebar.sessionRows` 与 `ClientMirrorStore.sessionSidebarSnapshot/threadUpsertSQL`：默认按有效活动时间降序、身份升序；连续两次 run_state 为 running 时保留原 sidebar_activity_at，避免流式刷新持续顶行。标题偏好仍由 ClientKit 的本地化比较处理。Tekes 的 SidebarColumnController 拥有外层导航与搜索，不拥有可复用会话行状态。

## 实现与接入

共享 `target=state` inventory 行增加 `projections` 对象，包含原 SessionProjection 已按 sequence 合并的值；原 summary 保持原样。没有新 RPC，不读取 harness 私有数据。旧 consumer 可以忽略新增字段。

SidebarBinding 继续订阅同一 shared inventory。标题优先读取 reconciled projections.title；兼容没有该字段的原 summary.projections.values.title，再兼容顶层 title/name。显式 null/空值代表清空，不回退到过期标题。未命名时沿用既有会话 ID fallback。

原 binding 的显示元数据记录有效活动时间；按 canonical ThreadActivity 的 running 状态冻结连续运行期间的位置，结束后使用当前 updatedAt，时间相同按原 session ID 升序。活动数据和全部行验证成功后才整体提交。查询投影继续保留这一库存顺序，不按命中返回顺序或标题再次排序。绑定不选择会话、不改 Run、草稿、操作或提交状态；宿主与第二 consumer 的原公开接入方式不变。

这是单个绑定生命周期内的显示排序。新建绑定/进程从当前 inventory 恢复时间；没有持久化 ClientKit 的 sidebar_activity_at，也没有宣称多窗口中途挂载的历史活动位置已完全对齐。用户标题排序、置顶区与自定义顺序、文件夹 outline、跨 endpoint 聚合、本地 FTS OR/BM25 仍待完成。

## 验证

- 默认 UI 全 workspace 357 项测试，含 36 项独立 consumer 检查；新回归通过公开 inventory 通知覆盖中文 projection 标题、旧版本不回退、显式清空、最近活动、连续运行冻结、结束重排及原草稿/选择保留。独立 consumer 实际消费每轮结果。
- 共享 web 服务 9 项测试；新增服务用例证明 canonical projection 独立发布、原 summary 不被重写。UI 与 web 全 target 严格 Clippy、fmt、边界检查和公开 pin 8包/199文件核验通过。
- 当前原生窗口 fixture 在失败→Retry→查询恢复→清空后，注入公开 projection/活动更新。截图确认中文标题首行，断言原 Button 实例、选择与草稿保留且无提交，窗口 exit0。连续运行冻结是组件/绑定回归证据，未声称已做原生冻结或物理 IME 验收。
- 当前二进制复验 Native Kernel0.9.1、Pi1.3.0、DSH0.2.1-rc.2，三端公开搜索/库存恢复/离屏显示均通过。采用隔离本地 provider，无付费调用；这些回执不证明三端原生阅读/分页或完整标题生命周期验收。

固定 a30 的上一阶段 379 项候选结果保留为原来源证据，本轮未重跑或提升该候选。上述 357 是当前默认公开 pin 的结果。完整 UI 目标、上游通用行帮助 #35 与最终 review/CI/合并/tag 仍未完成。

本阶段复用现有 target，incremental 关闭。共享 target 排他锁、lsof 与当前 metadata/依赖路径检查后，删除两个被当前构建替代快照的112个中间deps文件，释放207,114,240分配字节（约197.5MiB）；固定源码、截图、日志、收据与验收二进制保留。详见同名 JSON。
