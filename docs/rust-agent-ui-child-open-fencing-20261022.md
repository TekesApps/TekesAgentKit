# 子 pane 打开回包的 owner 隔离（2026-10-22，实验状态）

对照只读 ClientKit `ClientSidePanelHostView.swift` 的 tab 内容保留/关闭退役和 observation
generation，以及 Tekes `ClientAgentHostAdapters.swift` 的原 scope 导航。本轮修复 AgentKit demo
的异步结果安装顺序，不修改外部参考仓库、TekesUI 或协议/presentation 状态机。

原宿主在检查导航请求是否仍当前之前，已将同 ID 的旧子代理结果写入缓存，并 retire 较新的原
ChildPaneBinding。选择虽然没有跳走，当前 pane 的实际 owner 已被替换。另一个尚未安装的旧
source 可在用户关闭 tab 后重新进入目录。两个回归在修复前失败，日志保留；首版关闭用例误把
既有待发 ReadChild 命令当成关闭产生的命令，已先排空原命令后复现真正的 tab 复活断言。

现在按 `(panel parent, child ID)` 保存宿主安装代次与关闭失效标记，在替换或 retire 原 owner
之前拒绝旧/重复交付。关闭标记使用原导航 revision，覆盖关闭前尚未安装、但比当前 body 更晚
发起的请求。新的显式打开可以重开相同业务 ID。此表是窗口生命周期内的回包 fence，
不是另一套 ChildRun/pointer/执行状态；尚未实现生产文档缓存预算或完整请求队列排序。

仍允许原请求的有效非当前会话结果缓存到原 parent，不能夺取当前导航或焦点。选择的
main/panel/projection/IME 校验继续复用原机制。原生宿主只为已进入缓存的确切 source
订阅变化，不给被拒绝的不同 owner 增加观察者。

`ComponentDemo::install_child_source` 的返回 bool 继续表示是否选中，不等于是否缓存。
`false` 可能是有效的非当前缓存，也可能是拒绝；宿主应通过 `cached_child_source` 的原
source identity 判断是否接收，不可仅凭返回 false 丢弃有效 cache 或给拒绝结果订阅。
关闭不取消 Agent 执行，pointer/草稿/历史的 authority 仍由原 binding 持有。

## 验证

- UI workspace 505项通过，1项既有 #49 Popover 接收门禁显式忽略；严格 all-target Clippy、
  fmt、build、边界及实际8包/241文件公开源清单通过。包含独立 public API consumer 的全部
  回归；本轮没有改变共享 SidePanel API 或要求第二 consumer 复制 demo 导航表。
- 新增两项宿主回归：旧结果不得替换新 owner（同时检查重复交付、原编辑器 ID 和标题）；
  关闭必须拒绝尚未安装的更晚请求，且允许新的显式 reopen。
  既有有效非当前 parent 缓存、preedit 屏障及 owner drop 回归保持通过。
- 同一冻结程序 `.runtime-fixtures/AgentKitChildOpenFencing20261022.app`，SHA256
  `75e84a1360b86a0268f7f5c6d513d63eb607d90758f034403a81ba3dc141d911`：
  `--panel-navigation-selftest` 通过原 TaskPaneBinding 与异步创建的独立 ChildPaneBinding，
  在原生窗口按受控乱序交付旧/新结果和重复结果，再通过原关闭按钮验证 pending 结果不能
  复活 tab。当前 source、child 编辑器 ID、原 Task 编辑器、主草稿及无提交断言通过。
- 同一程序 `--readonly-reading-native-selftest` 通过 bounded public SEP 38→75 条分页、
  刷新/前插锚点、原分页/刷新控件、主草稿，以及 main/child 合成 preedit 下的双向原复制
  控件和私有测试剪贴板回读。分页/发送屏障保持，未生成可写的有界子历史 Composer。
- 同一程序通过真实 Native0.9.3、Pi1.3.0、DSH0.2.1-rc.2 公开数据到离屏 GPU 路径；
  Native 明确为 UI-only，不证明完整 lifecycle/archive、三端原生乱序或真实 bounded child 验收。

本轮没有重跑系统拼音；此前失焦失败保持未验收。合成文本服务、系统拼音软件事件、实体键盘
和候选窗/VoiceOver 分别记录，不能互相替代。跨进程 UI 尚待 TekesUI #48；交互 Popover #49
仍 OPEN。公开精确 pin 仍为 `09b4bab44b6dfa65487250ea816b610b9df59095`，未修改基础实现。

原始成功/失败日志、程序/源码哈希、前台轨迹和收据在 `.runtime-fixtures/child-open-fencing-20261022/`，
摘要见[机器记录](rust-agent-ui-child-open-fencing-20261022.json)。复用现有 target，未创建 target，
未删除缓存，释放空间0。冻结验收程序及全部证据保留。本阶段不合并、不打 tag，整个目标仍未完成。
