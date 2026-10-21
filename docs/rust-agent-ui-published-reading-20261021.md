# 已发布 reading 依赖与原生显式选择 reveal

默认 TekesUI 精确固定 `1294987dacabb33c0e411c13ee5ea544a1123f8e`，对应公开不可变 tag `remediation-checkpoint-20261021`。上游 PR40 和 main CI 均通过。AgentKit 原生 demo 现消费原 SDK 的稳定 item reveal，使显式选中的 Sidebar 行进入可见区域；不移动 Composer 焦点或手算滚动坐标。AgentKit UI 仍为实验 `0.0.0 / publish=false`，完整持续阅读及分页锚点尚未完成。

## 依赖与所有权

实际 Cargo 解析8个SDK包，审阅清单241文件覆盖这些包的全部文件、35个build-script输入、literal include输入及Cargo.toml/Cargo.lock/LICENSE。`check-ui-consumer-inputs.py` 校验实际Git checkout字节和完整消费包清单。新公开源全部2690个blob与Git tree逐文件核对；源码使用CoW克隆共享相同文件数据，变化文件来自匿名raw，不使用活动工作树。旧8cf/2ee清单与固定输入保留。

2ee是本轮最先取得的公开reading源码，独立路径接收方曾通过27项reading/rail/Start/focus/padding/help/Sidebar回归、原包装/独立consumer/native build、完整429项和严格Clippy。这是当时9b85f5b输入及明确适配下的证据，不替代最终Git pin验收。最终发布检查点只在消费范围增加Toolbar及其测试，已单独审阅并重新验证正式Git依赖。

Native AX桥补充转发SDK的required语义；独立consumer明确依赖原controls以接收reading token、options和text-host trait。静态白名单仅增加controls，禁止renderer/platform/harness进入consumer或bindings的边界不变。ProcessContainer View的远端keyboard/scroll/IME/AX仍未完整交付，保留现有真实编辑器与交互链路。

## 公开API与原生宿主

`reveal_sidebar_selection(host, target, version, options, text)` 验证原 `SidebarVisibilityTarget` 的来源、库存/导航/disclosure版本、挂载和一次性claim，随后调用原SDK `reveal_scroll_reading_item`。宿主传入捕获的原reading token；函数不替换文档、不修复像素、不操作编辑器焦点。SDK返回值和错误原样保留；一次已claim的拒绝需要新显式导航，刷新不自动重试。

原生demo在成功布局后消费显式目标，创建navigation-only reading attachment，经原SDK布局、reveal及最终重绘后发布AX/GPU画面。原分页、置顶、页外选中补入和手动折叠fencing继续复用。

**navigation-only attachment只声明当前显式目标，不是完整库存或Transcript的持续reading事务。** 宿主已有完整reading文档时应把原token传给公共adapter，不能复制demo附件覆盖它。持续item目录、追加/前插/分页事务、重连阅读位置和向上阅读保持仍需接通。当前不能宣称原生分页锚点或原生视口外Sidebar目标已验收；后者的视口外→可见证据目前来自真实CoreText组件布局测试。

## 已执行验收

- 正式Git pin：默认UI402项（含第二consumer42项）、fmt、all-target严格Clippy、静态边界、实际依赖输入校验和native build通过。
- 组件/布局：原补入目标从视口外进入可见；保持原焦点、中文marked text、选区和baseline；旧捕获不改reading token，重复请求拒绝，普通页数仍5。
- 正常.app：Sidebar显式reveal日志为Completed；恢复/失败保留/选中补读、原native preedit/focus/baseline、零提交断言通过。程序hash、日志和截图在同名JSON中。
- Native Kernel0.9.1、Pi1.3.0、DSH0.2.1-rc.2：真实协议/查询/库存/内容回传通过；三端原生窗口的流式单次发送、保留输出身份/选区/焦点、草稿隔离、重连防重放，以及取消、provider错误、显式恢复均通过。模型provider为隔离受控服务，无收费模型调用。这不等于三端原生历史锚点通过。
- 系统IME：普通离散按键nihao产生带分词的ni hao marked态，发送/切换/模型禁用；Enter结束组字并保留nihao草稿，未发送。再次组字加空格提交你好；B中wo加空格提交我。A/B恢复分别为nihao你好与我，界面历史仍20条。未调用PreeditRange/Commit API、未改输入法设置；这是系统IME自动化按键证据，未计人工硬件键盘、VoiceOver或全部输入法/选区组合。

SDK截图子进程曾失败，最终通过Cua取得正式组件窗口和真实系统组字截图/AX。截图工具曾在探针退出后重开无参数旧测试宿主，已关闭，未作为正式组件验收。直接typeText混合Unicode串只保留ASCII的独立观察交给TekesUI #41定位；系统拼音正常的补充已同步，不据此认定一般中文输入故障，不在AgentKit补偿键事件。

## 磁盘与后续

所有验证复用现有target。终态确认进程/占用、持单一Cargo flock后，删除明确归属旧快照的406个中间文件；随后依据两次全部fresh的Cargo artifact图，删除86个已被1294替代的2ee编译产物。du减少与去重已分配块一致，共1,108,123,648字节（1056.8MiB）。源码/WIP/固定快照/原清单/日志/截图、当前全部产物和native/reading/独立consumer验收程序保留；无新增target。共享target仍约20GiB，含现用依赖及无法从相对depfile确认归属的旧变体，未据体积删除。

下一步把RunView已有prepare/validate/apply publication计划接进原SDK持续reading事务，再验追加/前插/分页/阅读位置；之后推进剩余同名Views、菜单/管理入口及整体原生验收。PR10保持草稿，未完成最终review/合并/阶段tag，未标记整体目标完成。

首次6136086核心CI的Rust检查通过；Python边界测试仍构造旧consumer dev-dependency布局而失败。已同步该夹具并增加platform/harness仍拒绝的子用例，本地26项Python测试通过；运行时代码和以上原生程序未变。失败日志保留，新提交CI另核对。
