# #17 / #27：失败子 Run 公开重连恢复结果

按[契约](rust-agent-ui-child-reconnect-contract-20261024.md)，在 ba076cc 上补 opt-in Python 验收；默认 Pi18/DSH21 命令不变，不修改 Rust/UI、生产接口或精确 TekesUI pin。关联 PR #10，[结构化收据](rust-agent-ui-child-reconnect-results-20261024.json)与原协议帧保留于 SSD `.runtime-fixtures/child-reconnect-20261024/`。

最终真实 Pi1.3.0 **27条**、DSH0.2.1-rc.2 **30条**公开命令、共15次合成 loopback 请求 accepted。原 running→failed/error 两帧之后 reconnect，新父 publication 保存 freshChild，再公开 open child，重新读取父来源和子 Transcript。Pi epoch2→5、DSH5→8；同新 epoch、原父/工具来源/pointer 不变，原 User 位置、opaque turn ID、错误/时间/结算/fork 内容恢复。最终公开 parent-bound childHistory 内容也配对，两个 headless 与 owned DSH 退出0。没有付费模型、私有历史读取或桌面操作。

首次真实尝试保持 rejected：Pi 在历史重开后不再带直播 nativeStreamId，DSH 新父帧中的 child reader 尚未 ready。前者只从持久内容比较中排除这一精确订阅字段；其余 metadata 改变仍拒绝。后者通过公开 open child 后的第二份父帧验证，不放宽可读来源要求。发现 pointer 的第一帧不宣称 child 失败状态已恢复；原始失败、各 source 帧和进程退出证据保留。

worker 候选的重复来源解析、父子 epoch 不一致、局部 import 和命令字段错误已在主会话审查中纠正。候选最后5项测试中2项测试代码错误另保留；主会话修正后补跨 scope history 与临时 metadata 回归，最终定向7项、全 Python **249/0**，边界/矩阵/diff检查通过。合成 driver 只证明脚本控制流；旧帧复制并推进 epoch 的候选回归不算真实重连。真实结论只来自 `final/run-receipt.json`。

同一 child 主 Run 的恢复前后 ID 必须稳定；不同 scope 的 history 按来源定位并跟随其自己的 opaque turn ID，不推导 ID 格式或关系。本次实际两个路径 ID 相同，但 childHistory 仍是单独后读，不能称与子 publication 同帧。其内容成功也不代表 SidePanel 绑定/控制权限已验收。

程序复用共享 target 中 SHA256 `6aad53eb9910cb190ed5e887d27b1dd28169fea3245c1ed8f5eb27cb46a1215a`，归档只作相同 SHA 参考；运行依赖和程序前后摘要一致。Rust/UI/Cargo 输入不变，复用 f7fe185 的 root371/feature279/headless4/UI750/0/1及 demo/独立 consumer；本轮未调用 Cargo、未新建 target。ba076cc 的[两份 CI](rust-agent-ui-ci-ba076cc-20261024.md)已取回原 logs/artifact：consumer通过，唯一失败仍为#53。新提交CI另验，不合并/tag/发布。

实际 SidePanel 子 pane 使用 parent thread + child session 和原 for_child controller/祖先权限，需要独立捕获其原 publication；本轮明确 primary-child-run-only。Native新profile、完整分页、pane草稿/生命周期、桌面/物理IME/OS AX/ProcessContainer、最终布局与完整对齐仍 not_run，#17/#27继续开放。源码增量/输入manifest、原失败、日志、运行时快照与验收程序引用保留；本切片缓存删除0 bytes，不重复计算此前 SSD 迁移释放空间。
