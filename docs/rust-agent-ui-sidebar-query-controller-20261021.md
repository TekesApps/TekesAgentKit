# Sidebar 单端搜索：共享 reader 与公开 RPC

接续 [footer 显示层](rust-agent-ui-sidebar-search-footer-20261021.md)。本轮将真实查询状态放入 **不依赖渲染器的 presentation**，UI binding 只公开转接，不让宿主重写查询状态机。仍未完成整个 Sidebar 对齐。

## 契约与所有权

- `tekes_agent_contract::{SessionSearchResults, SessionSearchHit}` 对齐当前 ClientKit `SessionSearch.swift`：items、sessionId、snippet 与明确的 hasMore。缺少 hasMore、错误类型或空 sessionId 报错，不变成“成功空结果”。未知 provider 字段保留在 extra，原结果也保留在 publication 中。
- `SessionRuntime::search_sessions_at_epoch()` 只读调用已声明的 `session.search`，同时校验 actor 入场和返回时的原连接 epoch/能力。只有这个已定义的读取操作在调用方离开后取消 reader；未将取消规则扩展到发送、归档等 mutation，也不调用 Agent cancel。
- `presentation::SidebarSearchController` 拥有原 query、250ms 去抖、尝试代数、连接/能力变化、显式 Retry 和关闭。`ui_bindings::SidebarSearchBinding` 是同一实现的公开别名。组件继续消费相同的 key/input/phase/intent 类型。
- `new_with_messages()` 接收产品不可用/不支持文案。控制器没有主题、字体、窗口或焦点实现；基础组件和样式继续来自精确固定的 TekesUI。

只搜索显式选择的 endpoint，不暗中扫描其它 harness。不变的 query 不重复请求；A → B → A 使用新代数；空白 query 立即撤销旧尝试。连接身份改变会重新读取当前 query，旧回复不能在新 epoch 下发布。这是读取恢复，不重放 prompt 或其它 mutation。连接通知在同一状态锁内读取当前 query 与实时身份，不能携带早先缓存的 query 覆盖用户新输入；已应用的身份通知不重复发起读取，`reconcile_connection()` 也遵循同一规则。

命中去重保留 provider 原顺序，同 ID 的最后一个 snippet/元数据生效，地址总是来自原 endpoint。不会对返回成员再加标题过滤、私读 harness 数据库或伪造本地全文索引。`unresolved` 保留尚未进入当前 inventory 的身份；后来 canonical inventory 到达时可更新解析状态，不触发额外搜索 RPC。**主动刷新未知命中的公开 inventory baseline 尚未补齐**，没有把命中直接变成可写会话事实。

## 控件接入与生命周期

控制器与 view 的 revision 生命周期分别处理：

1. 创建 `SidebarSearchBinding`，将产品搜索框的 query 传给 `set_query()`，订阅 publication。
2. 首次挂载或显式切换来源，用 `Sidebar::replace_search_source(source.owner(), &snapshot.input)` 建立来源边界；新控制器可从 revision 1 开始。
3. 后续只用 `update_search_from(source.owner(), &snapshot.input)`。即使旧来源晚到的 revision 更大，或清空后的 key 为 None，也不能覆盖新来源。
4. 将 `take_search_intents()` 的原 Retry 交回原 binding 的 `retry()`。控制器再次校验 owner、endpoint、epoch、代数、失败阶段和能力；重复/旧 Retry 不发起新请求。

原来的无来源 `update_search()` 仍供明确的单一 publication fixture 使用；绑定控制器后不能绕过来源检查。显式替换来源会淘汰旧 action ID，失败的替换保留当前树。`ComponentDemo` 提供对应的 bind/update 转接。第二 consumer 的 `new_sidebar_search` / `mount_sidebar_search` / `sync_sidebar_search` 通过这些公开接口工作，没有私有依赖或另写状态机。

实验 demo 增加 `--sidebar-query <text>`，可与显式 `--endpoint-config` 组合。它只安装当前 endpoint 的共享 reader。产品搜索框、整体 toolbar 和来源选择仍由宿主提供；查询策略和结果状态由 AgentKit 维护。

## Pi adapter 修正

Pi 的公开搜索现在明确返回 hasMore：最多返回 100 个匹配，仅额外发现第 101 个匹配才确认还有更多；references.sessions 仍保留原 20 条引用候选接口。按既有 inventory 的 native 文件修改时间确定搜索顺序。标题命中不启动 Pi 进程；未物化/缺少 native 文件的会话不因搜索而重新创建。正文只经公开 get_entries 获取当前分支，匹配 timeline 文本，不匹配 JSON 的 id/model 等元数据、reasoning 或已关联工具结果。命中片段围绕正文位置生成，大小写展开的偏移映射回原始 UTF-8。

这些不是完整 Pi 搜索语义验收：与 Foundation 的 diacritic-insensitive 匹配、Swift 字素计数的精确片段边界及图片生成引用等仍需补齐对照。未将 Unicode scalar 范围声称为字素等价。

## 验证入口与证据边界

`--components --transcript-content --sidebar-query-selftest` 使用 owned in-process SEP fixture：真实鼠标 Retry → 共享控制器 → 公开只读 RPC → 排序命中 → 清空；核对旧 Retry 拒绝、原 inventory 控件、中文草稿和无 submission。窗口截图/收据仍不证明物理 IME、VoiceOver、真实 harness 索引或完整布局。

`scripts/test-real-session-endpoints.py --ui-components --ui-transcript-content --ui-sidebar-query --ui-demo <binary> ...` 对明确的 Native/Pi/DSH runtime 各建隔离会话，通过本地合成 provider 写入唯一历史标记，再用共享 reader 查询并验证原 endpoint/目标、typed hasMore、无 UI submission 和渲染后的正式会话列对齐。这是**真实 harness 传输与离屏 GPU 证据**，不替代原生窗口/阅读锚点验收。

DSH 0.2.1-rc.2 的 Web 默认将全文索引设为 openAt=never，RPC 能明确失败；客户端保留该失败，不伪造空结果。只有传入测试开关时，隔离实例才按官方配置将 `session-query-sqlite` 的内存索引设为 first-search；不改已安装部署、用户数据库或厂商代码。内容搜索用历史内容标记验证，不把 rename metadata 假定成可检索文档。

Native 测试固定干净的 Kernel v0.9.1 / ae5d6fdf；supervisor、worker、helper、workspace-service 构建到 AgentKit 自己的 target，Kernel 源码保持只读。Pi 使用显式暂存的 1.3.0 CLI，DSH 保持 0.2.1-rc.2；精确身份与检查结果见 [同名收据](rust-agent-ui-sidebar-query-controller-20261021.json)。

## 排版与样式约束

本轮不增加独立皮肤。消息的左右方向、单条输入的分组及气泡外 footnote 是 AgentKit 的排版约束；颜色、字体、圆角、交互反馈和语义间距继承 TekesUI 的默认组件与 tokens。显式升级固定的 UI 版本后，基础样式应统一反映到所有 Agent 组件，不能靠局部覆盖保留旧外观。

## 尚待完成

多 endpoint 聚合、完整搜索结果成员关系/FTS 语义、未知 inventory 命中的主动公开刷新、snippet/help、搜索 header/count、项目分组及完整 outline/status-pane 布局仍由 AgentKit 完成。不会要求宿主各自实现这些业务状态。默认混合阅读与其它 Views 的清单继续保持开放；本轮不合并、发布或升级 TekesUI pin。
