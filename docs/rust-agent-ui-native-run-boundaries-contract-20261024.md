# #17 / #21：Native 显式 run 的结算与消息归属

基线 AgentKit6318bf6，对照 committed ClientKit c289bc9 与 Tekes8858ccd，三个外部实现只读。独立源码复核确认两个缺口：显式 started 尚无匹配 settled 时，Rust 会因 is_streaming=false 或换到下一 run 而推断终态；原 settled 分支未按 data.id 绑定，可能结束另一 run 并错挂 fork boundary。#17 的失败累计已完成，但不能据此宣称显式边界也已对齐。

本切片沿用原 Native projection 与 settle，不新增运行状态机。先按字符串 run ID 收集已有公开 settled 事实，再投影原消息/工具归属，最后只为已建立且有 matching settlement 的显式 run 结算。终态累计该 run 的明确 assistant error/aborted，marker aborted 同样有权取消，取消优先。原最终答案、工具停止、fork boundary 都由对应 run 的原 settled 位置决定，不能只修改 status 标签。

started 的字符串 ID 建立显式身份，与瞬时 streaming 分开。缺少 matching settled 的显式 run 保持 running，不产生 ended_at/fork boundary/最终答案，也不停止其工具。后续 started 建立新身份；另一 ID 的迟到 settled 只结算原 run，不影响当前 run 或其消息归属。settled 后仍保留显式模式，后续 user/assistant 属于最近 started，直到下一个 started，与当前 ClientKit 一致。

缺少或非字符串 ID 的 marker 不能创建 partial 显式身份；未知 settled 不创建幽灵 turn。为与现有 ClientKit 对齐，空字符串 ID 仍按字符串处理，不偷偷增加验证策略。settled 预扫描可匹配稍后出现的同 ID started；多个 settled 使用最后一处位置/时间，任意 aborted 保留。只有 settled、没有 matching started 时不建立显式 run，已有 legacy 消息仍走原推断。相同 ID 重复 started 的畸形 producer 不在本次扩展协议范围。

先红/绿验证非 streaming 缺结算、ID错配/缺失/未知、跨 run 迟到、工具/fork 隔离、settled 后消息、重复 settled、正常匹配与 legacy 保留。仅有marker而没有保留消息时，按ClientKit不输出空turn或rail导航目标；缺ID的started不能改成partial显式身份。matching settlement后实时preview的既有路径对照当前ClientKit，双方均沿用activeTurnID而未额外检查settled；本轮保留原行为，不新增偏离契约的过滤器，也不宣称该交错已被真实producer验证。真实 harness、组件挂载、静态/回归和桌面分层记录，不把合成历史等同于真实生命周期或可见 UI。

主会话协调新 worker，仅修改原投影与定向回归；主会话审查并串行验证 root/feature/UI/普通 demo/独立公开 consumer。所有构建使用 SSD 共享 target，保留失败、源码和验收程序。精确公开 TekesUI50ec49f不变；公开main7c13ec0仅有无关图标更新，#47/#48/#49/#53/#54尚无本任务对应公开交付，不凭本地分支升级。桌面/截图/物理IME/OS AX/ProcessContainer均不运行，#53严格门不豁免，不合并/tag/发布。
