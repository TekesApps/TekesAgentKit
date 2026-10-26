# #17 / #27：真实子 Run Transcript 两帧来源

基线 f7fe185。当前已提交 ClientKit c289bc9 的原 turn/消息来源与 Tekes8858ccd 的宿主归属作为只读参考；不修改任何外部仓库或生产接口。既有真实协议仅捕获父 Summary 的三份 presentationSource，终态 childHistory 不是原子 Run 的运行中/失败两帧，因此本次补验收路径。

在现有 runner 显式增加 `--child-transcript-source`，默认命令和结论不变。沿用当前已编译、与 f7fe185 输入绑定的 feature headless 程序，先核对 SHA 与已有归档，只执行共享 target 中的当前程序，不执行历史归档或重新编译。固定 Pi1.3.0/DSH0.2.1-rc.2、隔离 owned home/data、合成 loopback provider 与原公开命令保持原边界。

次序：原父 running source 验证通过且 provider 请求已进入 held gate → 子 Run 的 presentationSource running 验证通过 → 显式 release HTTP400 → 原 waitSettled/Pi childRead 的 pendingInputs=0 → 原父 failed source → 子 Run 的 presentationSource failed。父三帧与子两帧分别归档；不能把不同 publication 称同帧，也不从 provider 日志推断 Transcript 状态。子身份来自原父 Summary pointer，不拆 opaque ID 或猜 session UUID。

每份子见证核对原 projectionSucceeded/evidenceScope、Ready/可读、epoch、Run 与 selection 的完整 scope、非 stale。只消费同一份 authority 的 child NativeConversation/Journal，复用已有 narrow transition predicate 确认当前 authored input 与运行/错误边界；不复制通用状态机。Pi 原 User 按 entryId/ordinal/recordId 配对，DSH 按原 Journal sequence/sourceSequence 配对；item 可见、非 provisional，inputPresentation 的 itemId/turnId 必须指向该原 item。

真实首轮发现DSH直接选择child后的`SessionProjection.summary`可缺省，这是原Rust契约的Option字段。仅子Transcript profile显式允许DSH这一缺省，原run_open、可读Journal、当前User/turn边界、actionables和queue仍必须通过；摘要存在时仍核对其running/等待事实。原数据不补空对象或running=false，收据记录inventorySummaryPresent=false。Pi与默认父Summary profile不放宽，缺省仍拒绝；这不是新增库存或导航权限。

由 User 的完整 opaque turnId 查唯一原 TranscriptTurn：running 时 status=running/endedAt缺省，failed时同turn且status=error/endedAt有值；错误 item 必须配对同份原 Native error assistant 或 DSH turn/end error，不能只凭相同文本或 errorMessage 猜造来源。两份子帧要求同 scope、epoch、authored input/source与turnId，revision严格递增；不要求父/子帧相同revision，不宣称服务端跨流事务一致。failed仍有dispatching时拒绝，不清队列制造结果。

正例与拒绝回归覆盖错scope、父快照、sourcePosition/recordId、重复/错turn关联、假running、缺当前错误、旧输入、stale/epoch/投影失败、队列和revision不增。合成fixture不当作真实证据；真实runner、输入脚本、精确二进制/运行时、原帧和退出结果另封存。不把JSON一致性检查称producer防伪。

本切片只改Python验收脚本，不改Rust/UI/SDK；运行定向及全Python后做真实协议。Rust生产输入、精确TekesUI50ec49f与原验收程序不变时复用已有root/UI/demo/第二consumer证据，不重新构建或新建target。不能据此声明原生pane动作、草稿完整生命周期、布局/glyph、桌面/物理IME/OS AX/ProcessContainer或完整Transcript完成，相关Issue继续开放，#53严格门不豁免。
