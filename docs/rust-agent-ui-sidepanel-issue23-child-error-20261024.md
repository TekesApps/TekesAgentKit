# Issue #23: Native child-open error dispatch regression

The native demo host now routes `HostResult::ChildOpened(Err)` through a small pure admission helper. It returns the error for the existing `Host::failure` notice path only while `ComponentDemo::child_request_current` accepts the originating `PanelRequest`; a retired request produces no notice. Successful child-open handling and the component navigation state machine are unchanged.

`ui/demo/tests/native_child_error_dispatch.rs` creates real `ComponentDemo` navigation state, verifies a current request yields the host error notice, then changes the selected parent and verifies the same request is rejected as stale. The integration exercises the exact helper called by the native host branch without starting a native window.

The coordinating session ran the direct test: 1 passed, 0 failed, 0 ignored (`.runtime-fixtures/sidebar-host-20261024/retained-rank-focused-final-tests.log`). Desktop validation remains not run. This regression proves the request admission and notice forwarding decision; it does not prove rendered notice appearance in a live native window.
