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
