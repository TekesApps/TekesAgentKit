# #17 / #23：原 Native 子 pane 的累计分页与恢复来源

关联 #27、PR #10；功能起点 `91129d7`。这是下一切片的设计，不是已完成验证。复用现有公开 Kernel 0.9.3、原 SameSession ChildRun、FamilyCatalog/OpenChild/ChildRead/Earlier admission；不改 Kernel、TekesUI、ClientKit，不读 harness 私有文件，不启动桌面。

既有 `BoundedChildSourceWitness` 只证明 head-only，并且应继续拒绝累计旧页。单独保存公开分页 RPC 收据只能说明请求过哪些页，不能证明它们进入原 pane 的本次 publication。新增独立 opt-in 累计来源接口，沿原 bounded mutex 读取、合并与投影，不重建状态机、不为了见证清空缓存或重复查询。

## 结构与冻结边界

沿用默认关闭的 `acceptance-source-witness`。presentation 新增 `BoundedChildPagedSourceWitness` 及 `ChildRun::transcript_with_bounded_paged_source_witness`，binding 新增 `BoundedChildPagedPaneWitness` 及 `RunBinding::snapshot_with_bounded_paged_source_witness`。原 head-only API 与普通 snapshot 保持语义。binding 仍走原 source/scope、owner、epoch、document 与单次 publication 分配，不把另一次普通 snapshot 与原始页拼接。

记录实际操作顺序：InitialHead、RefreshHead、Earlier、PrefixValidation 的原 typed request/response、读取 epoch、是否 reset、是否实际合并、有效 has_more_before；prefix validation 仅验证，不作为 entries 合并。来源链在原成功合并分支同步提交；失败不能生成成功 merge。原 mutex 内完成 refresh、原 transcript 和来源冻结；显式见证再按记录重放验证 entries、有效分页标记与原 head/source-call 元数据，验证结果不能替换正常投影。

普通 refresh 不复制整个累计 entries 来维护审计，也不重放来源；仅保留本次页与操作元信息。可以用 Arc 共享不可变页。来源最多保留128个 receipt、8 MiB编码数据；超限/无法完整记录后，累计见证返回明确不可用，正常投影继续，不静默截断后宣称完整。原实现真正 reset 到新 head 时可开启新的完整链。既有 transcript() 自身的累计复制不是本切片新开销；不宣称性能验收。

## 失效与重连

- 相同cut/revision保留原缓存与链；新head覆盖重叠条目，标明实际顺序。
- 前进时，原prefix验证成功且无gap才保留旧链。gap、history-stale等导致原reset时，新链只能引用真实新head。
- Earlier保留实际 through_sequence/history_revision/before/page_size，不额外发RPC。过期epoch响应和失败投影不得返回成功见证。
- 重连不因验收清缓存。新epoch publication可以明确引用旧epoch已缓存页，但每页原epoch必须可见；不声称旧页全部经新连接重新读取。旧intent及owner/lease沿原门禁拒绝，显式refresh后再分页。
- `server_transactional_freshness_proven=false`。证明一次本地读取的原投影，不证明网络字节或服务端事务。

## 实施与验收

presentation实现者负责 bounded_history.rs、child.rs、导出与专属测试；bindings实现者随后负责新见证接口与publication/owner测试。主会话串行整合原demo的Earlier/ChildRead、真实Native runner/验收器、构建和Issue回填；子会话不运行Cargo，避免共享target并发。

原控件的验收执行入口为feature-only `ChildPaneBinding::read_intent_with_paged_source_witness`；它与普通read_intent共享admission/claim和Earlier执行，随后直接取同次累计见证，不能先普通snapshot再另读来源。8 MiB指保留的编码记录量，不承诺单页捕获/编码时的瞬时内存峰值。

先完成公开fixture回归：累计Earlier成功且head-only仍拒绝；无额外RPC；增量重叠/prefix验证；gap/stale reset；epoch等待拒绝与跨epoch缓存明确标记；owner退休/迟到拒绝；来源不完整/超限只拒绝见证而普通snapshot与缓存可用。定向通过后主会话再跑适用root/UI默认与feature、严格检查及第二公开consumer。

再由同一原Native pane执行真实多页、显式重连和恢复；把原typed请求/响应、source/scope、epoch、publication、frozen cut、mounted transcript及父草稿/选择配对，保留原程序/SHA/失败/收据。验证结果按实际范围回填 #17/#23/#27，不以合成回归替代真实harness，不以CPU挂载替代桌面或滚动视觉验收；#18阅读主线、SDK #53/#42及完整交付仍独立开放。
