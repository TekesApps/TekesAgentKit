# 派生 pane 与只读历史的来源契约

状态：开发中；关联 PR #10。只修改 Rust AgentKit；ClientKit/Tekes 作只读行为参照。
TekesUI 的输入、焦点、IME、阅读事务与渲染不变，桌面验收暂缓。

## 父来源权限（#12）

ClientKit 的 ClientEmbeddedRunView 在移除时停止 pane，ClientAgentTaskView 按当前任务行替换
对应 RunScope。Rust 派生 child/Task 也必须保持原来源，而非只在创建前检查一次父来源。

绑定捕获原 TaskRunGuard、child weak lifetime 和 SideChatController pointer。它们是原权限的
引用，不是新的任务/执行/草稿状态机；不强引用旧 view。来源去重后传给派生绑定，避免嵌套
时重复扩张。Missing 拒绝派生安装及旧回调；Closing/Unknown 保留可读状态并关闭控制门禁。
前后两次校验包住异步 history baseline。主 Run 的 mount 卸载仍不等于取消远端或丢弃其草稿。

for_run 明确继承父来源；低层 open(runtime, scope, ...) 仍表示独立显式来源。
Task 的同目标 presentation 共享、独立 history lease、Task generation/ABA 门禁均保留。

## 只读历史实例身份（#13）

问题：scope/publication/epoch 只表示数据地址与读顺序，两个独立绑定可能同时为同 scope 的
publication=1。旧视图的 ReadOnlyTranscriptIntent 或 PaneSnapshot 不能交给新绑定继续使用。

引入不携带状态的 ReadOnlyTranscriptSource（opaque Arc identity）：

1. 每个 RunBinding 有自己的实例身份；只读 snapshot 携带该身份。
2. ChildPaneBinding::readonly_input 校验 snapshot 的实例身份，再输出原组件输入。
3. ReadOnlyTranscriptInput 可携带该来源；独立展示输入允许 None，但不能获得 endpoint 读取权限。
4. ReadOnlyTranscript 在成功应用原 frame 后保留来源，显式读取 intent 捕获同一来源。
5. ChildPaneBinding::read_intent 在 claim/IO 前校验实例身份；错投请求不消耗正确来源的原 gesture。
6. 原 scope、publication、epoch、一次性 claim、分页策略与 SDK reading token 继续分别校验。

这只是业务来源 provenance，不替代 SessionEndpoint 权限、SDK 控件/mount 身份、阅读事务或
持久 checkpoint。不按 scope 构造“等价”身份，不建立全局 counter/registry，不改写 publication。
新字段属于尚未发布 UI API；同步所有公开 consumer 字面量和接入文档。正常绑定输入由
readonly_input 统一生成，宿主不用复制身份状态。

## 验收

父来源：retire/drop、A→B→A、跨 scope Task、SideChat Closing/Unknown/absence、异步 baseline、
准备票据失败清理、主 Run 草稿不变；共享包/demo/第二公开 consumer。

只读来源：同 scope/epoch/publication 的独立实例拒绝交叉 snapshot 与 intent，旧 source 退役后
不转交新 source；错误调用不消费原 gesture；原来源仍可刷新/分页，clone 只执行一次，重连刷新
和分页 epoch 门禁不退化。使用共享 target；组件与公开 consumer 检查后执行完整 UI/严格检查。

不声称本轮覆盖原生窗口、物理 IME、VoiceOver 或 ProcessContainer；不放宽 #47/#48/#53 门禁。
