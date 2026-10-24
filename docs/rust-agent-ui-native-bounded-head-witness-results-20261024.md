# #17 / #23：原有界 head 与原 pane publication

按[契约](rust-agent-ui-native-bounded-head-witness-contract-20261024.md)，基线 `9039d5f` 增加默认关闭的 presentation/绑定验收接口；没有改 Kernel、TekesUI、ClientKit、Tekes、锁文件或既有精确 pin。

`ChildRun::transcript_with_bounded_source_witness()` 在原 bounded mutex 中复用原 refresh/投影，返回 Transcript 与原 typed head 请求/响应、父 scope、child 和页大小。`RunBinding::snapshot_with_bounded_source_witness()` 经过与普通 snapshot 相同的构造路径，返回这一读取的原 PaneSnapshot 和来源；publication只分配一次，原 source、scope、权限、epoch/connection与document检查共用。controller的RunSourceWitness仍拒绝只读来源。

本接口只证明 **head-only**：若保留的旧页或自然增量会使投影超出原head，或累计分页标记与原head不同，就拒绝见证并保留缓存。普通读取不清空、不重查、不缩小历史来凑证明。raw head是原typed响应，不是网络字节；refresh可能另做prefix校验，不宣称单RPC或服务端事务一致。

主会话修正候选中的普通路径全量历史复制，普通refresh仍按原方式增量合并；head-only分支检查额外保留的键，所有重叠行仍由原head完整覆盖。默认feature关闭时不保存raw head。只作代码与回归判断，没有性能基准结论。

## 验证

| 层级 | 结果 |
| --- | --- |
| 新presentation定向 | 6通过 |
| bindings定向 | 6通过，含新增4项 |
| Root默认 / feature | 373/0/0；287/0/0 |
| UI默认 / bindings+demo feature | 757/0/1；652/0/0 |
| 严格Clippy / 格式 | 默认与feature通过 |
| 普通 / feature demo | 构建通过，未启动 |
| 独立公开consumer | 默认/feature实际执行通过；新增接口作方法/类型检查 |
| 边界 / 验收矩阵 | 通过，未改变旧验收状态 |

定向测试通过公开bounded RPC验证单次原head读取、原request与response、error/stopped原phase及投影；旧页/自然增量拒绝、缓存保留、自定义/独立来源拒绝、读失败及跨epoch响应均拒绝成功见证。绑定测试核对原source身份、pub1→见证pub2→普通pub3、组件input直接借用该返回快照的Transcript、父草稿保留、旧controller接口仍拒绝只读、退休无IO、读取中退休与读取失败不返回成功。

首次严格Clippy发现测试里的反向迭代写法，修正成rfind后完整重跑通过；两个中间lint日志与候选源码保留。最终5文件哈希、程序、测试/构建/consumer日志及固定输入位于 `.runtime-fixtures/native-bounded-head-witness-20261024/`，见[JSON](rust-agent-ui-native-bounded-head-witness-results-20261024.json)。Cargo串行复用SSD shared-target，没有新target、删除0字节；源码、验收程序/证据和仍需的缓存保留。

前两提交CI已回填：20b4c44的push40104896916/PR40104900090、9039d5f的push40105849539/PR40105854722，常规checks、bindings/demo opt-in、独立consumer全部通过，唯一失败仍#53严格阅读，CI demo跳过。两轮原日志/artifact、137/243来源闭包及PR merge父关系均已核对。不能将这些CI当本轮新headCI。

## 未证明的范围

这是接口、合成公开协议和组件输入的验证，**还没有真实Native同一原pane的running→failed双帧**。累计分页来源、完整恢复/布局、桌面/物理IME/OS AX/ProcessContainer、完整ClientKit和性能仍未验收。下一步在原demo/owned Kernel路径消费此接口，使用原控制与原数据回传；#17/#23保持open，#53不豁免，不合并/tag/发布。
