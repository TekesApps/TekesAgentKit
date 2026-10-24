# #17 / #27：失败子 Run 的公开重连恢复

基线 ba076cc；只读参考 ClientKit c289bc9 的 SessionWindowCoordinator.reconnect（同 address、preservingLoadedHistory），Tekes 8858ccd 的宿主边界。先补现有公开协议验收，不改 Rust 生产代码、TekesUI 或依赖。

在现有转换 runner 增加显式 `--child-transcript-reconnect`，要求同时启用 `--child-transcript-source`。默认命令与旧结论不变。原 running→failed 两帧及 childHistory 完成后，执行 reconnect → open parent → 新父 presentationSource 并从该帧保存 freshChild pointer → open freshChild → 再读父 presentationSource → 子 presentationSource → parent-bound childHistory。

发现 pointer 的首份新父帧只作发现，不宣称其 child reader 已完成恢复；公开 open child 后再读父帧，复用原 failed 来源验证。发现帧、可读父帧与子帧必须同新 epoch，pointer 必须一致；缺少可读来源继续拒绝。Pi 直播 error metadata 的 nativeStreamId 是订阅标记，历史重开可缺省；只忽略该精确临时字段，其余错误内容、来源、时间和结算边界仍精确核对。

新父帧必须重新通过原来源验证，pointer、原工具来源与父身份不变，connection epoch 严格增长，不假定加一。子恢复帧必须使用新 epoch、新父帧公开的 pointer，保持原 authored User 来源、opaque turn ID、failed/error 状态和原错误/结算/fork 内容。历史恢复另核对原返回值，不用它替代 publication 的来源证明。保留原帧、命令、收据、进程退出和失败证据。

同一主 Run 恢复前后的 opaque turn ID 必须不变；不同 scope 的 ChildRun history 按原 User 来源定位，再跟随其自身 opaque turn ID，不能要求或推导两种 scope 的 ID 关系。本次实际两条路径的 ID 相同，也不据此授予 pane authority。

此处 presentationSource 打开的是 `RunScope::main(child)`，收据限定为 primary-child-run-only。实际 SidePanel binding 使用 parent thread + child session；其完整恢复、pane authority、控件和草稿不在本切片验收范围，不得用本次成功替代。也不证明服务端跨流事务一致、完整分页、桌面/IME/AX或 ProcessContainer。

先做定向拒绝回归与全 Python 测试，再以原精确 SHA 的共享 SSD target 程序运行 owned Pi/DSH 和合成 loopback provider。核对程序及运行依赖前后摘要，不重新编译、新建 target、读取私有历史或使用真实模型。Rust/UI 输入不变时复用既有测试，不重复制造缓存。#53 严格阅读门槛保持，Issue 不据此整体关闭。
