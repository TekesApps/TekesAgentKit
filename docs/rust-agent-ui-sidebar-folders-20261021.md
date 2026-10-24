# Primary Folder 分组、筛选与折叠

> 历史阶段记录：其中“pins 不受 query/folder filter 影响”的解释已由 2026-10-24 的[当前会话 Sidebar 成员关系契约](rust-agent-ui-sidebar-query-membership-20261024.md)取代。原程序/测试/截图保持历史归属，不能证明新语义；新切片的本地名称 FTS、排名与真实窗口验收仍开放。

本记录固定分组阶段的源码及验收；后续已补已知/随后恢复行的 [显式导航临时展开](rust-agent-ui-sidebar-navigation-20261021.md)，独立选中行补读和滚动可见性仍未完成。

本轮推进当前 ClientKit `ClientSessionSidebar.sessionRows/setSessionGrouping/filterSessions/folderTitle` 的分组、筛选和持久折叠部分：普通会话按 Primary Folder 分组，分组顺序取排序后首次出现顺序；置顶区独立于分组和筛选。Tekes 宿主仍拥有外层导航、系统路径和工作区适配。本轮未修改 ClientKit、Tekes、Kernel 或 TekesUI。

## 公开输入

- `SidebarRow.primary_folder_id` 为可选的宿主明确输入；有 ID 时优先于路径，路径变化不改变分组身份。没有 ID 时按非空原会话 `folder_path` 分组，无路径时归入 No Folder。`SidebarFolderKey` 用 endpoint 和 `Id/Path/None` 类型区分身份，不拼接字符串，不把文件夹身份当作 SessionAddress 或 workspace 路由。
- `SidebarBinding` 继续只从原公开 summary 的 `cwd` 读取原目录，`primary_folder_id` 默认 None。宿主可在同一版本化行 publication 中提供 Primary Folder ID/path；不猜测 harness 扩展字段，不读取私有数据库，不把当前执行目录当作原目录。
- `SidebarPreferences.group_by_folder` 和 `collapsed_folders` 属于同一 AgentKit 偏好服务。新组默认展开，已折叠组在取消分组、重启或暂时消失后仍记住折叠偏好。
- `SidebarInput.folder_filter` 是明确的宿主输入，仅筛选普通行；`sidebar_outline`、`Sidebar::update` 负责统一算法。筛选不导航、不切换 Run、不修改草稿。`SidebarPublication::project_search_for_display` 同时保留宿主的 selection、filter 和 view revision，避免搜索投影覆盖筛选。
- 组标题取原路径最后一段，无路径时使用可本地化的 `SidebarLabels.no_folder`；排序仍要求宿主提供系统本地化比较。显式 folder ID 优先，不对路径做文件系统查询或平台外的路由推断。

`SidebarOutline`、`SidebarFolderGroup`、`SidebarFolderKey`、`SidebarFolderIdentity`、`sidebar_outline`、`sidebar_folder_title` 和组 header/body ID 查询均为公开 API。实验 UI 直接构造行/输入的调用者需增加 `primary_folder_id` 与 `folder_filter`（无值时 None）；绑定工厂自动填入默认值。公开第二 consumer 只依赖两个 UI 包即可消费这些类型。

## 组件与回调所有权

普通组复用 Kit 已有 `SummarySectionView` 业务组合，以及 TekesUI 原 CompoundButton、Disclosure 状态、Stack、Text 和 ContainerLayout。组内行使用库 spacing/inset 缩进；默认 Summary 的缩进仍为0。没有新树引擎、命中/焦点/IME/滚动实现或 painter。

组 disclosure 的原动作映射到 `SidebarPreferenceChange { expected, value }`；宿主仍调用同一 `SidebarBinding::set_preferences`。偏好 revision 或写权限变化会退休旧 header 回调，CAS/ABA/写盘失败行为沿用原服务。单纯目录显示更新保留同一 ID 组的 header；分组/取消分组、折叠/展开保留持续存在的原会话控件。筛选撤下的行则退休旧 action/owner，恢复时不能复用旧回调。

只读偏好输入仍显示原分组与折叠状态，header 不提供写入口。折叠后由原 Content 活跃子树机制隐藏叶子语义，恢复时保留原 Button；组动作不被转换成会话选择。实际 Composer 组字期间，宿主的既有 IME 门禁仍拦截 header 导航；数据更新/筛选不会改写 preedit、选区、baseline 或焦点。

## 持久格式边界

偏好文件由格式3升级为4，1/2/3在原文件锁内原子迁移。3的原 sort、pin order、pinned expansion、revision 和 pin 成员关系保留，新增分组默认关闭、collapsed folders 默认空。未知格式、空身份或重复 collapsed folder 在发布前拒绝；账户/环境隔离、0600权限及写盘失败不发布成功继续沿用。

旧二进制不能读取格式4。本轮实验宿主及测试使用隔离路径，未迁移用户安装版本的配置；尚未发布新的 headless 契约或 UI 包。后续发布需按此实际格式边界同步升级。

## 体验与未完成项

原生窗口已验证原搜索失败/实际 Retry/恢复，原公开库存目录通知、共享 pin 动作、启用分组、实际 header 点击折叠、筛选保留全局 pin，以及取消分组恢复。三张截图已查看：独立置顶区、中文组标题、缩进叶子、折叠隐藏和筛选后的单组均可见。原 a Button、选择和中文草稿保留，无提交。

第一次原生探针在窗口尚未 active 时提前失败，二进制和日志保留；探针改为在既有总超时内等待 SDK 实际 active 状态后继续，第二次完整退出0。没有伪造焦点状态，也没有修改 SDK。工具驱动的窗口不是物理 IME、VoiceOver或全产品体验验收。

当前公开 SDK 的 Disclosure 只有状态模型，组合层缺少标准方向指示器；此处不画字符三角或自制箭头。[TekesUI #38](https://github.com/TekesApps/TekesUI/issues/38) 请求原组合触发器、默认指示器和单一动作/语义 owner。当前功能性折叠已接通，标准指示器和整体体验仍待补齐。偏好确认会退休旧 header action ID，触发器的键盘焦点连续性尚未验收；正式接入需由原 SDK 控件及其事件时记录保留 owner/焦点并拒绝旧意图，不在 Kit 手工补焦点。

排序/分组/筛选选项菜单仍待 #36；按文件夹新建会话所需的 workspace resolver/来源回调、完整 Primary Folder 管理目录、分页/选中行补齐与导航时展开所选组、多端库存聚合和本地 FTS 仍未完成。整体 Run/Transcript/SidePanel 等 Views、真实 harness 原生阅读/分页、物理 IME/Escape 和 VoiceOver 继续在完整目标范围内。默认公开 TekesUI pin 保持8cf0840，固定336候选仍单独验证，PR #10 保持草稿。

具体测试终态、来源、原生二进制/截图哈希及缓存清理收据见同名 JSON，不以窄场景通过代表完整 UI 交付。

本轮默认 UI workspace 372 项、固定336候选399项、独立公开 consumer 40项、无UI workspace186项通过，两边 all-target 严格Clippy通过；固定候选生成器的26项既有探针、wrapper及native build也通过。Python工具26项、fmt、所有权边界与默认8包/199文件精确依赖检查通过。最终默认二进制与原生v2验收二进制哈希一致，并另通过 Native Kernel0.9.1、Pi1.3.0、DSH0.2.1-rc.2 的独立进程公开查询与内容离屏回传；未进行三端原生分组/阅读验收。初次全UI执行有一个旧格式3断言、一次测试代码Clippy失败，修正后重新跑完整UI检查；失败日志保留，无UI源码未再变化的已通过检查没有重复执行。

所有自有验证终态后，核对进程、排他Cargo锁、lsof与当前metadata/依赖文件，删除上一阶段被替代的私有receiver的56个中间编译文件，释放105,414,656字节（约100.5 MiB分配空间）。没有新建独立target，失败/成功二进制哈希不变；源码、固定快照、WIP、日志、收据、截图及现用/用途不明确的缓存保留。
