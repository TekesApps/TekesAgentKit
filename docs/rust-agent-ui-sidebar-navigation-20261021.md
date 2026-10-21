# Sidebar 选中导航与临时展开

本轮对齐 ClientKit `reconcileSelectedThread` 与 `handleActivate` 的区别：显式导航可以展开所选组，但普通库存刷新不能再次展开用户刚收起的组。导航展开属于单个 Sidebar 的临时视图状态，不改写持久 `collapsed_folders` 或 `pinned_expanded`。Kernel、协议和持久格式4均未改变。

## 调用契约

组件新增公开 `SidebarSelectionNavigation { revision, selected }` 和 `Sidebar::reconcile_navigation`。每个 Sidebar 生命周期只接受一个宿主导航版本权威；先发布匹配的选择，再在真实导航发生时调用：

```rust
sidebar.update(&input)?;
sidebar.reconcile_navigation(&SidebarSelectionNavigation {
    revision: host_navigation_revision,
    selected: input.selected.clone(),
})?;
```

普通 watch、库存/标题刷新、重连、偏好发布和筛选变化只更新输入，不重新生成导航事件。revision 必须非零且单调递增；低版本、同版本不同目标、以及与当前选择不匹配的新版本请求在修改前拒绝。同版本同目标幂等返回 false，不因重复交付而再次展开。

新请求遇到尚未恢复的库存或缺失的目标行会等待，`pending_navigation()` 返回该请求的副本。后续合法行发布只消耗一次；选择变化取消过时请求。等待期间原文件夹或置顶区的手动 disclosure 操作会压过该请求，晚到的目标行不能重新打开用户刚收起的区段。

普通会话仅在没有 folder filter 或匹配 filter 时展开原文件夹；不会清除读者筛选。置顶会话则展开独立置顶区，仍不改持久偏好。API 返回 true 表示一个新请求已消耗（包括清空选择、或目标被筛选挡住），false 表示等待目标或幂等重复。它不创建结果行、不做选中行网络补读、不调整滚动位置，也不赋予新的执行能力。

## 手动操作与无变化确认

导航临时展开了一个持久偏好已经标记为 collapsed 的组时，用户再收起它，正确的持久值仍然是 collapsed。组件按当前有效展开状态计算目标值，移除该区段的临时展开，并让晚到的相同导航保持幂等；不能按旧持久值再反转一次。

宿主在派发 `take_preference_intents` 后必须回送同一个来源的最新偏好确认，即使 store 返回成功但值/revision 没变化，或写入失败。`preferences_current` 会识别本地待确认状态；回送旧权威值可回滚失败操作，成功无变化确认则刷新下一次的回调目标。例：

```rust
let result = source.set_preferences(&change);
if let Some(confirmed) = source.preferences() {
    sidebar.update_preferences(&confirmed, true)?;
}
result?;
```

同一版本化偏好中的无关 pin 顺序变化不抹掉导航展开；真实区段偏好变化仍具有权威。更新、临时展开和手动确认会退休旧 header 回调，持续存在的会话叶子仍保留原 Button。标准 trigger 的 ID/键盘焦点连续性仍待 TekesUI #38；不在 Kit 手工补焦点。原输入、焦点、命中与渲染继续由 SDK 负责。

## Demo 与验证边界

ComponentDemo 的显式 `select` 和清空选择复用其既有导航版本，调用同一个公开组件方法。重复 `sync` 不被当作导航；切换会话继续使用原 RunBinding/Composer 草稿，组件仅记录临时展开请求。宿主决定初始选择是否算导航，组件不会从普通构造/刷新推断一次导航。

定向测试覆盖导航一次性、无变化的手动收起确认、旧/冲突/错误目标、恢复等待、筛选、等待期间用户操作压过晚结果、权威回滚、无关偏好变化、置顶独立展开和实际 demo 会话草稿。第二公开 consumer 独立消费原 API；现有组字测试还核对 actual select 被 IME 门禁拦截，纯显示展开/筛选不改 preedit、选区、baseline 和焦点。

扩展原生 fixture 先完成上一轮分组/折叠/筛选，再实际点击原导航按钮展开所选组、点击原 header 手动收起、发布普通标题刷新并确认仍收起，最后再次点击导航展开。持久偏好快照和 revision 全程不变，原叶子保留、中文草稿保留，无提交。三张导航截图已查看。第一次在分组准备 phase50 超时，日志、二进制及当时探针源码保留，根因未确定；扩展场景改用30秒总保护并补等待诊断，第二次完整退出0，不把复跑成功当作生产根因修复。

原生窗口为工具驱动的自有 fixture；不是物理 IME、VoiceOver、实际三个 harness 的原生导航/阅读或完整产品体验。未改变默认精确 TekesUI pin8cf0840，固定336候选单独复验。完整选中行补读/分页、滚动可见性、按文件夹新建/管理、多端、菜单和其余 Views/最终 UI 验收仍未完成；PR #10 保持草稿。

具体测试终态、固定来源、二进制及截图哈希、失败记录与缓存清理见同名 JSON。

本轮默认 UI workspace380项、固定336候选407项（保留既有26项候选回归与wrapper检查），其中第二公开consumer41项；两边all-target严格Clippy和native build通过。Python工具26项、fmt、静态边界与默认8包/199文件精确源检查通过。当前默认二进制与原生v2二进制哈希一致，另通过Native Kernel0.9.1/Pi1.3.0/DSH0.2.1-rc.2的公开查询和内容离屏回传。无UI核心未变，其186项和严格Clippy沿用上一已提交阶段，不宣称本轮重跑或三端原生导航验收。

所有自有验证终态后，检查进程、排他Cargo锁、lsof和当前metadata/依赖文件，删除被本轮替代的私有folder receiver的56个中间编译文件，释放106,381,312字节（约101.5 MiB分配空间）。复用原target，无新独立构建缓存；源码、固定快照、WIP、日志、收据、截图、失败/成功可执行文件及现用/用途不明确的缓存保留。
