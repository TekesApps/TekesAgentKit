# TekesUI #17公共编辑基础（2026-10-18）

显式开发清单：eb2f5bc0dae0ce051c56b5aca0d2bb31d600f4e7be4be08d48d82bc0076effb0；不是发布pin。原有AgentText现在转发key_event；native host启用TextKey来源并保留全部修饰键/up/repeat。UI15、headless76、strictClippy及Rust边界通过。未启动AgentKit原生窗口。

TekesUI新增原ContentHost的编辑确认、组字开始/焦点代次屏障、Tab策略和提交意图。原TextSurface最低高度和reveal已修正；独立包395共享/平台、consumer74、feature编辑16项通过；新增72实际token场景。Gallery原生/Metal Light换行、Tab及CommandEnter意图和默认导航分开留证；物理IME/VoiceOver未验收，性能不测。

本demo业务策略仍是显式按钮发送。未把库提交意图直接映射为业务发送；多行Composer/键盘提交需随#18受控草稿和#20视口接入，#21随后统一动作来源。#17纵向/行边界命令及物理验收继续跟踪原Issue。

TekesUI `design/reviews/editor-keys-20261018/patchset` 包含独立可构建公共补丁、单独AgentKit桥接补丁、原WIP依赖及canonical-runtime-source.tar.gz。canonical版本与本清单逐字节一致；standalone测试包有显式子工作区Cargo调整，勿混作消费源。没有提交、推送、tag或关闭Issue；完整#15–#21交付后再移交本会话验收。
