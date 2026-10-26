# Sidebar 独立选中行读取（2026-10-21）

对照 ClientKit `ClientMirrorStore.sessionSidebarSnapshot` 的 selected 查询、`mergingSelectedThread` 和导航/筛选分离行为，补齐 Rust 端从同一规范内存投影独立读取选中会话的通路。Tekes 继续负责窗口和路由；AgentKit 负责查询适配与 Sidebar 显示；没有改 ClientKit、Kernel、Tekes 或 TekesUI。

## 公共 API

- `SharedAgent::read_thread_inventory(endpoint, session_id)` 返回一个 `ThreadInventoryRead`：端点/会话身份、连接代次、库存恢复状态、连接状态、规范 item 及共享 actions。仅复制该会话的 summary/projections 和显示事实；不复制整份 store 或 journal，不打开 history lease，不调用 provider，不触发库存刷新。批量库存和此读取共用元数据构造和归档排除规则。
- `SidebarBinding::read_selected(address, selection_generation)` 保留原 binding/endpoint 生命周期及规范读取。宿主为每个选择作用域提供正数 generation。
- `SidebarSelectedRead::project_into(source, display, generation)` 在原 source、代次、选择 generation、规范事实都仍匹配时补入真实普通行；不更改 selection、revision、filter 或偏好。未恢复/断开、未知/已归档、已有行、搜索成员集合、其他文件夹和全局 pin 均不会被独立选中行污染。返回 false 是未应用，不等于恢复成功。
- `has_active_row(source)` 用于导航有效性，独立于列表成员规则：允许打开真实 active 会话，同时保持读者的 filter/search/pin 布局。过期读取及归档会话不可因旧列表仍留有一行而重新打开。
- 独立公开 consumer 提供 `supplement_selected_sidebar`，保留原 source/read/generation，不重新解析当前选择。

宿主保留独立显示 revision，示意：

```rust
let read = source.read_selected(&address, selection_generation)?;
let mut display = source.snapshot().input(Some(address));
display.revision = host_display_revision; // 与同一 Sidebar 已发布输入单调一致
read.project_into(&source, &mut display, selection_generation);
sidebar.update(&display)?;
```

`read_thread_inventory` 使用短投影读取锁。宿主 pin/method/capability 回调在锁外收集；锁内只构造规范事实并生成 actions，避免适配器重入造成锁顺序问题。读取在收集宿主元数据期间跨过连接代次时直接拒绝；不重试或执行任何 RPC。宿主偏好与 runtime 是分离的权威，不能将这一读取宣称为跨存储事务。

## Demo 接入

ComponentDemo 新增显式 `set_sidebar_reader(Arc<SidebarBinding>)`；安装及后续绑定必须共享同一 runtime，不能只比较端点字符串。原生 demo 装配同一个 SidebarBinding。既有导航版本映射为读取 generation，不增加第二套选择状态机。

普通库存显示缺少当前选择时，sync 使用独立读取补入，再交给原 Sidebar 的排序/分页/分组/临时 disclosure。搜索仍使用原查询投影；筛选不被清除。异步 OpenSession 完成时重新检查规范 active 行：暂未出现在显示库存中可以打开；权威状态已归档则拒绝，即使旧显示库存仍有该行。已授权新建的既有路径保持原创建规则。

没有安装 reader 的自定义宿主仍只消费其显式 publication，不能从宿主外部猜测 summary。规范内存中完全未知的会话仍不制造标题、文件夹、pin 或 actions；需要新的公开库存事实，由既有显式刷新策略提供，而不是 UI 自行读取 harness 文件。

## 验证范围

定向测试覆盖：不刷新/不打开历史的补入、原 actions/title/cwd、重复去重、选择 ABA、异端点/异 runtime、重连失效、title clear、归档排除、未知行、原 filter/search/restoration、pin 独立性、旧显示行不能复活已归档目标、真实 demo 异步导航与两份中文草稿/无重复提交。

库存恢复原生探针保留“恢复未完成不是空”和失败保留行/淘汰旧 ID 的检查。其空 publication 阶段现在独立读取真实选中会话，不再将已知 active 的选中行伪装为空；增加原输入事件生成的中文 marked text、focus/编辑 baseline 和无提交检查。原生探针是受控 fixture 和工具驱动 preedit，不能代替物理输入法或真实三端原生验收。执行状态、截图、二进制和精确测试结果见同名 JSON，未执行项明确记录。

默认公开 TekesUI 仍为8cf；固定336候选只做独立回归。#22/#28 公开版本阻塞未改变，未接入默认自动 reveal，也未复制滚动/焦点实现。PR #10 仍为草稿，完整 UI/真实阅读分页锚点/物理 IME 与 VoiceOver/最终合并 tag 尚未完成。

本轮最终核心187项、默认UI393项、固定候选421项、第二公开consumer42项及两边严格Clippy/native build通过。最终默认程序通过Native Kernel0.9.1/Pi1.3.0/DSH0.2.1-rc.2公开查询/库存/内容离屏复验。正常.app启动的原生断言通过，窗口随后关闭；首次裸程序启动phase0超时保留，根因未确定。两次截图命令未取到图像，故本轮不计视觉验收。修正锁范围期间的旧候选回放因输入哈希改变而作废，最终固定输入重新完整通过；失败收据不算验收。

终态后检查进程、lsof并持有共享target单一排他Cargo锁，删除此前paging receiver和本轮被替代receiver的420个中间文件，`du`与去重inode释放量均为320,397,312字节（305.6 MiB）。未新增独立target；源码、固定快照、日志、收据、验收程序（含硬链接.app）及不明确缓存保留。

提交前新增的公开TekesUI heads `6d566a4774efba2c691bb8a596fc08cb2ecaf66a` / `1a7614a273e1a897ddc8a7ad8cb41d303a4a1290` 也已按源码树及匿名raw读取核对：它们是Scene3D PR39，尚无消费所需的composition模块和混合reading/reveal闭包，不能替换现用8cf消费pin。此结果不等于审查通过了这些版本用于其他组件。
