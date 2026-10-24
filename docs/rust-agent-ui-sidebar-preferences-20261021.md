# Sidebar 置顶区与显示偏好

本记录保留格式3阶段的源码和验收边界；后续分组、折叠和格式4迁移见 [Primary Folder 接入](rust-agent-ui-sidebar-folders-20261021.md)。

本轮对齐当前 ClientKit `ClientSessionSidebar.sessionRows/movePin`、Sidebar 的区段激活和 Sort by 偏好。Tekes 宿主仍只拥有外层导航/窗口。固定公开 TekesUI pin 保持 `8cf0840790334ba224afc0595897c74231c712e6`，没有修改 SDK、ClientKit、Tekes 或 Kernel。

## 业务行为和公开 API

- 置顶会话进入独立、可折叠的置顶区；普通会话位于区段之外，折叠不会吞掉后续会话。复用 Kit 已有区段组合及原 TekesUI CompoundButton/Disclosure/Stack，不另写焦点、命中、折叠引擎或绘制。持续存在的原会话控件在置顶、重排、折叠/恢复时保留。
- `SidebarPreferences` 包含 `sort`、`pin_order` 和 `pinned_expanded`；`SidebarPreferencesSnapshot` 携带持久 revision。排序不改变 pin 成员关系。已存 pin 顺序优先，未列出的当前置顶项按完整 SessionAddress 排序；普通最近活动继续消费原库存顺序。标题 A–Z/Z–A 只反转标题比较，等价标题仍按身份升序。
- 标题比较能力由宿主 `SidebarTitleComparator` 提供。当前 macOS demo 用 CoreFoundation 用户本地化、忽略大小写比较；常量和 C 签名对照本机 Apple SDK CFString.h。Kit 不用 lowercase/字节顺序冒充系统本地化排序；缺少比较器时拒绝标题排序输入。测试内的简单比较器只验证业务方向和身份 tie-break。
- `ordered_sidebar_rows` 提供相同业务排序；`Sidebar::update_preferences` 接受原快照和可写能力，`take_preference_intents` 返回带原 expected 快照的变更，`move_pin_change` 生成原地址的上下移动变更。原 `displayed_input` 仍返回完整已接受库存输入，不冒充经过分区的视觉行序列。
- 偏好变更或可写能力撤销/恢复会退休旧区段 action ID，晚动作不能绑定到新偏好。相同已接受输入不触碰 retained host 的可变树；本地 Disclosure 切换仍需原 store 的后续发布来确认或回滚。未安装偏好 store 时为只读展示，不伪造成功写入。

原 `ThreadPreferences` 是 AgentKit 的本地客户端偏好存储，继续由宿主提供私有路径与账户/环境 scope。它现保存 Sidebar 偏好，原 pin 关系和 RPC receipts 独立保留；`SharedAgent`/`SidebarBinding` 只暴露同一存储的公开读写，不新增查询状态机或 harness RPC。示例接入：

```rust
sidebar.set_title_comparator(host_localized_compare);
let input = source.preferences();
sidebar.update_preferences(&input.clone().unwrap_or_default(), input.is_some())?;
sidebar.update(&source.snapshot().input(selected))?;
// 从原 ContentHost 的事件边界取得 actions；变更保持原 SidebarBinding。
for change in sidebar.take_preference_intents(&actions) {
    source.set_preferences(&change)?;
}
```

存储格式由2升级为3，读取1/2时原子迁移；旧二进制不能读取3，发布/降级需要按此格式边界处理。本轮只修改实验源码，未发布新的 headless 契约版本。持久 CAS 在同一锁内核对原 revision 和值，拒绝旧回调及 ABA；写盘失败不发布内存或 watch 成功。账户/环境隔离、文件锁与0600权限继续沿用原服务。

## 仍未完成的菜单与 Sidebar 范围

[新 TekesUI #36](https://github.com/TekesApps/TekesUI/issues/36) 请求原 Menu 捕获激活时的稳定 value/path，以及子菜单、勾选和逐项 disabled 声明。当前固定 API 的 MenuContent.selected 是最后状态，ContentAction 不携带当时选中值；不轮询它来伪造每次选择，也不平铺大按钮替代源 App 的选项菜单。

因此本轮交付排序/置顶顺序的公开状态和业务回调，**原生 Sort/Group 选项菜单与 Move Pin Up/Down 入口尚未完成**。置顶区折叠已在真实窗口接通。文件夹 outline、跨端聚合、持久活动时钟、本地 FTS OR/BM25、#35 通用行帮助及其余完整 Views/最终原生验收继续保留在范围内。

## 验证分层

- UI全workspace 362项（独立public consumer37项），无UI全workspace183项；两边all-target严格Clippy通过。新用例覆盖pin/普通区排序、双向标题tie、原控件保留、折叠与旧回调退休、权限撤销恢复、公开pin动作→同一偏好存储→宿主和第二consumer。持久层另覆盖格式迁移、非默认偏好重启恢复、账户隔离、ABA、写盘失败与重复地址拒绝。
- 原生v3 fixture经原搜索失败/实际Retry/恢复/清空，再用公开pin动作置顶a/b，实际点击原区段。偏好revision从1写为2；折叠后原行从语义树隐藏，原Button实例、选择和中文草稿保留，无提交。截图已查看，窗口exit0。工具驱动的AppKit点击不等于物理鼠标、IME或VoiceOver验收；原生pin重排/标题菜单尚未验收。
- v1/v2原生测试均在phase6等待异步结果截图超时；保留旧基线的独立A/B二进制在当前环境也发生相同超时，不能据此归因于新置顶代码。测试改用SDK已有50ms WakeRedraw等待异步文件/点击，避免持续即时重绘占住主队列；v3后续完成。失败日志和二进制均保留，不宣称由此证明生产事件循环根因。
- 本轮最终二进制另复验 Native Kernel0.9.1/Pi1.3.0/DSH0.2.1-rc.2 的公开查询、库存恢复与离屏组件数据回传，三端通过；它们没有进行原生pin重排/菜单验收。a30候选仍保留上一阶段来源，没有重跑或提升。默认依赖未提升，PR保持草稿；最终review/CI/合并/tag仍需完成。

本轮复用现有UI/无UI target并关闭incremental，没有新建独立缓存目录。排他Cargo锁及当前metadata/依赖检查确认，已知被替代快照的中间依赖已在前轮清理，本轮匹配0项、删除0项、释放0字节；现用或用途不明确的缓存及所有失败/成功验收产物保留。完整哈希、终态与边界见同名JSON。
