# 多 endpoint SideChat 宿主接线设计

关联 #24，前置为 #22 的实际多 connector 宿主。当前首端 SideChat 可用、非匹配 endpoint 明确禁用；本设计补齐原控制器的按端组合，不另建 pointer、mutation、草稿或 Run 状态机。ClientKit/Tekes 只读参考，SDK 不改，桌面不跑。

## 权威与生命周期

每个原 SessionRuntime 恰好保留一个 SideChatBinding。创建、打开、关闭按捕获的 parent.endpoint_id 查原 binding；未知端明确失败，不回退首端。不同 endpoint 的相同 session ID 不能共用面板或控制器。支持状态仍由各端原 capability 得出。

BoundSideChatRun 额外保留创建它的原控制器身份，接收返回前要求原 owner 和当前 pointer 都仍匹配；相同 endpoint/session 的结构相等不能替代来源身份。已退休 pointer、另一个 controller 的旧返回不能挂载到当前源。同一宿主生命周期禁止把同端 controller 替换为另一 Arc；重复注册原 Arc 幂等。换源需创建新宿主，不能静默替换仍有请求的源。

ComponentDemo 保留按 endpoint 的原 binding 集合；同步某端 snapshot 只能清理该端已撤销的 panel_runs，不得用 A 端的空列表删除 B 端。切换父会话保留各端 canonical RunBinding 的草稿、附件和提交状态；跨端延迟结果可以保留有效原面板，但不能抢当前选择/焦点。现有 panel/navigation revision 与组字门禁继续生效。

宿主订阅全部原控制器的变化，仅唤醒共享同步；不逐帧轮询、不合成聚合成功状态。单端公开接入保持兼容，缺失端维持禁用。注册表只持有原 Arc，没有撤销/替换 API；捕获的 parent 地址路由到唯一 canonical source，不由当前选中端重定向。

## 分工与验证

独立子会话只处理 BoundSideChatRun 原 owner 身份和定向协议 fixture；主会话处理按端绑定、ComponentDemo/native 路由、订阅与集成审查。共享 target 串行构建。

计划验证：同 session ID 双端 create/open/close 原端归属、A/B 切换草稿/附件保留、原端 pointer 撤销不删除另一端、旧 source/迟到返回拒绝、未知端禁用、单端既有 suite 与第二公开 consumer。原生入口编译通过只证明接线；真实多 harness、窗口/物理中文 IME/AX 仍单列，不能以 fixture/离屏替代。

## 实现与最终验证

实现提交 `c7104ac`，PR #10。主会话集成；两个独立子会话分别提供原 owner 证明和双端回归，另经独立只读审查。注册表固定原 Arc，原单端 setter 委托注册；不同 Arc 的替换为宿主编程错误，新宿主使用可返回错误的 register_side_chat_binding。没有增加第二套 pointer、草稿或提交缓存。

`SideChatBinding::owns_run` 在安装前验证原 controller 和完整当前 pointer。ComponentDemo 每个 pane 按自身 endpoint 的原 owner 判断保留；native Create/Open/Close 查捕获 parent 的唯一 source，并订阅全部 source。成功回包只有实际缓存的 RunBinding 才安装 observer；Create/Open/Close 错误用原导航、面板、projection、composition 门禁检查，旧端错误不覆盖当前提示。

最终 UI workspace **657 passed / 0 failed / 1 既有忽略（TekesUI #49）**，严格 all-target Clippy、fmt 和静态边界通过。其中 SideChat binding 8 项、多端宿主 3 项、pane uploads 7 项、panel navigation 15 项与原多端 Sidebar 宿主 1 项通过。成功测试走实际 fixture RPC 与组件 AX 命令：同 parent/scratch ID 的不同 endpoint、草稿/附件/prepared ticket 保留、B 关闭只退休 B、A 迟到只缓存不抢选择/焦点、同端不同 controller 的相同 pointer 拒绝、未注册端禁用。

初次测试中的 creating 瞬态等待会错过立即完成的 RPC，已改为明确阻塞 fork 回包再取消、通过原 unknown-creation reconcile 安装相同 pointer；没有延长等待或取消校验。面板夹具改为复用 setup 注册的 canonical owner。准备中的草稿由原 prepared ticket 持有，验证取消后原中文文本/附件恢复，不能把准备时显示为空误判为丢草稿。所有失败尝试日志保留。

第二公开 workspace consumer 的 open_scratch_run 实际调用 owns_run，随完整 suite 验证正常重开、重连和拒绝退休 pointer；独立外部 package 另编译公开 owns_run API、运行无效配置门禁，不宣称它运行了有效 SideChat IO。普通 demo build 成功，未启动窗口，也未把前一多端搜索 PNG 转移为本提交的显示验收。

日志与程序归属保留于 `.runtime-fixtures/sidebar-host-20261024/`：`sidechat-multi-clippy.log`、`sidechat-multi-workspace-tests.log`、`sidechat-multi-and-owner-final-tests.log`、`public-sidechat-consumer-run.log`、`sidechat-multi-demo-build.log`、`sidechat-multi-source-manifest.json`。源码 hash 与归档程序可核验；[机器收据](rust-agent-ui-sidechat-multi-endpoint-20261024.json)包含验证层级。

未验证真实多 harness 组合、当前程序的系统中文输入/原生关闭确认/AX/窗口/chrome/ProcessContainer；DSH 无公开 discard 仍禁用。Issue #24 保持开放。构建复用共享 target，未新增 target、删除缓存或释放空间。
