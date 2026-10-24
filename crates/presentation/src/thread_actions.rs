//! Native ClientKit sidebar policy projected for every renderer, in one place.
use std::collections::BTreeSet;
use tekes_agent_contract::*;

pub struct ThreadActionContext<'a> {
    pub endpoint: &'a str,
    pub session_id: &'a str,
    pub kind: &'a str,
    pub url_scheme: &'a str,
    pub capabilities: &'a EndpointCapabilities,
    pub ready: bool,
    pub running: bool,
    pub archived: bool,
    pub pin: Option<&'a ThreadPinState>,
    pub host_methods: &'a BTreeSet<String>,
}
fn action(
    id: &str,
    label: &str,
    icon: &str,
    interaction: &str,
    reason: Option<&str>,
) -> ThreadActionDefinition {
    ThreadActionDefinition {
        id: id.into(),
        label: label.into(),
        icon: icon.into(),
        interaction: interaction.into(),
        enabled: reason.is_none(),
        disabled_reason: reason.map(String::from),
        copy_text: None,
    }
}
pub(crate) fn fork_action(
    capabilities: &EndpointCapabilities,
    ready: bool,
    running: bool,
) -> ThreadActionDefinition {
    let reason = if !ready {
        Some("Agent 尚未连接")
    } else if !capabilities.supports("session.fork") {
        Some("当前 Harness 不支持此操作")
    } else if running {
        Some("会话仍有运行或排队中的任务")
    } else {
        None
    };
    action("fork", "分叉会话", "fork", "execute", reason)
}
pub fn thread_actions(context: &ThreadActionContext<'_>) -> ThreadActionsPresentation {
    let c = context;
    let unavailable = if !c.ready {
        Some("Agent 尚未连接")
    } else {
        None
    };
    let capability = |method: &str| {
        unavailable
            .or_else(|| (!c.capabilities.supports(method)).then_some("当前 Harness 不支持此操作"))
    };
    let idle_capability = |method: &str| {
        capability(method).or_else(|| c.running.then_some("会话仍有运行或排队中的任务"))
    };
    let pin_support = {
        (c.pin.is_none() || !c.host_methods.contains("thread.setPinned"))
            .then_some("当前连接暂不支持置顶")
    };
    let pinned = c.pin.is_some_and(|p| p.pinned);
    let mut items = vec![action(
        if pinned { "unpin" } else { "pin" },
        if pinned { "取消置顶" } else { "置顶" },
        if pinned { "unpin" } else { "pin" },
        "execute",
        pin_support,
    )];
    if c.capabilities.supports("session.rename") {
        items.push(action("rename", "重命名", "rename", "rename", unavailable));
    }
    items.push(fork_action(c.capabilities, c.ready, c.running));
    let mut copy = action("copySessionId", "复制会话 ID", "copy", "copy", None);
    let raw = if c.kind == "dsh" {
        c.session_id
            .strip_prefix("dsh:thread:")
            .unwrap_or(c.session_id)
    } else {
        c.session_id
    };
    let backend = if c.kind == "pi" {
        "pi-stdio-rpc"
    } else {
        c.kind
    };
    copy.copy_text = Some(format!("{}://{backend}/{raw}", c.url_scheme));
    items.push(copy);
    if c.archived {
        if c.capabilities.supports_unarchive_session && c.capabilities.supports("session.unarchive")
        {
            items.push(action(
                "unarchive",
                "取消归档",
                "unarchive",
                "execute",
                unavailable,
            ));
        }
    } else {
        let reason = idle_capability("session.archive").or_else(|| {
            (!c.capabilities.supports_archive_session).then_some("当前 Harness 不支持归档")
        });
        items.push(action("archive", "归档会话", "archive", "execute", reason));
    }
    ThreadActionsPresentation {
        endpoint: c.endpoint.into(),
        session_id: c.session_id.into(),
        label: "会话操作".into(),
        items,
    }
}

pub enum ThreadActionCommand {
    Rename(String),
    Fork,
    Archive,
    Unarchive,
    Host,
}
pub fn increased_fork_title(title: &str) -> String {
    for (opening, closing) in [('(', ')'), ('（', '）')] {
        if let Some(stripped) = title.strip_suffix(closing)
            && let Some((prefix, digits)) = stripped.rsplit_once(opening)
            && !digits.is_empty()
            && digits.bytes().all(|c| c.is_ascii_digit())
        {
            let mut digits = digits.trim_start_matches('0').as_bytes().to_vec();
            if digits.is_empty() {
                digits.push(b'0');
            }
            let mut carry = true;
            for digit in digits.iter_mut().rev() {
                if *digit == b'9' {
                    *digit = b'0';
                } else {
                    *digit += 1;
                    carry = false;
                    break;
                }
            }
            if carry {
                digits.insert(0, b'1');
            }
            return format!(
                "{prefix}{opening}{}{closing}",
                String::from_utf8(digits).expect("ASCII digits")
            );
        }
    }
    format!("{title} (1)")
}
pub fn thread_action_command(
    presentation: &ThreadActionsPresentation,
    request: &ThreadActionRequest,
) -> Result<ThreadActionCommand> {
    require(
        presentation.session_id == request.session_id,
        "thread action target mismatch",
    )?;
    let action = presentation
        .items
        .iter()
        .find(|a| a.id == request.action_id)
        .ok_or_else(|| Error::Unsupported("unknown thread action".into()))?;
    require(
        action.enabled,
        action
            .disabled_reason
            .as_deref()
            .unwrap_or("thread action unavailable"),
    )?;
    Ok(match action.id.as_str() {
        "rename" => {
            let title = request.title.as_deref().unwrap_or("").trim();
            require(
                !title.is_empty() && title.chars().count() <= 300,
                "invalid thread title",
            )?;
            ThreadActionCommand::Rename(title.into())
        }
        "fork" => ThreadActionCommand::Fork,
        "archive" => ThreadActionCommand::Archive,
        "unarchive" => ThreadActionCommand::Unarchive,
        "pin" | "unpin" => ThreadActionCommand::Host,
        _ => {
            return Err(Error::Unsupported(
                "thread action requires a client gesture".into(),
            ));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn caps() -> EndpointCapabilities {
        let mut caps = EndpointCapabilities::methods(&[
            "session.rename",
            "session.fork",
            "session.archive",
            "session.unarchive",
        ]);
        caps.supports_archive_session = true;
        caps.supports_unarchive_session = true;
        caps
    }
    #[test]
    fn native_menu_order_and_clipboard_identity_are_shared() {
        let capabilities = caps();
        let methods = BTreeSet::new();
        let context = ThreadActionContext {
            endpoint: "dsh",
            session_id: "dsh:thread:123",
            kind: "dsh",
            url_scheme: "tekes",
            capabilities: &capabilities,
            ready: true,
            running: false,
            archived: false,
            pin: None,
            host_methods: &methods,
        };
        let result = thread_actions(&context);
        assert_eq!(
            result
                .items
                .iter()
                .map(|i| i.id.as_str())
                .collect::<Vec<_>>(),
            ["pin", "rename", "fork", "copySessionId", "archive"]
        );
        assert!(!result.items[0].enabled);
        assert!(
            result.items[0]
                .disabled_reason
                .as_deref()
                .unwrap()
                .contains("置顶")
        );
        assert_eq!(
            result.items[3].copy_text.as_deref(),
            Some("tekes://dsh/123")
        );
        let pi = thread_actions(&ThreadActionContext {
            kind: "pi",
            ..context
        });
        assert_eq!(
            pi.items[3].copy_text.as_deref(),
            Some("tekes://pi-stdio-rpc/dsh:thread:123")
        );
    }
    #[test]
    fn pinned_order_and_archive_variant_come_from_authority() {
        let capabilities = caps();
        let methods = BTreeSet::from(["thread.setPinned".into()]);
        let pin = ThreadPinState { pinned: true };
        let context = ThreadActionContext {
            endpoint: "k",
            session_id: "s",
            kind: "tekes",
            url_scheme: "tekes",
            capabilities: &capabilities,
            ready: true,
            running: false,
            archived: true,
            pin: Some(&pin),
            host_methods: &methods,
        };
        let result = thread_actions(&context);
        assert_eq!(
            result
                .items
                .iter()
                .map(|i| i.id.as_str())
                .collect::<Vec<_>>(),
            ["unpin", "rename", "fork", "copySessionId", "unarchive"]
        );
        assert!(result.items[0].enabled);
        assert!(result.items[4].enabled);
        let request = ThreadActionRequest {
            session_id: "s".into(),
            action_id: "movePinUp".into(),
            title: None,
        };
        assert!(thread_action_command(&result, &request).is_err());
    }
    #[test]
    fn fresh_policy_rejects_stale_commands_unknown_actions_and_wrong_targets() {
        let capabilities = caps();
        let methods = BTreeSet::new();
        let context = ThreadActionContext {
            endpoint: "k",
            session_id: "s",
            kind: "tekes",
            url_scheme: "agent",
            capabilities: &capabilities,
            ready: true,
            running: false,
            archived: false,
            pin: None,
            host_methods: &methods,
        };
        let idle = thread_actions(&context);
        let request = ThreadActionRequest {
            session_id: "s".into(),
            action_id: "archive".into(),
            title: None,
        };
        assert!(matches!(
            thread_action_command(&idle, &request).unwrap(),
            ThreadActionCommand::Archive
        ));
        let active = thread_actions(&ThreadActionContext {
            running: true,
            ..context
        });
        assert!(thread_action_command(&active, &request).is_err());
        assert!(
            thread_action_command(
                &idle,
                &ThreadActionRequest {
                    session_id: "other".into(),
                    ..request.clone()
                }
            )
            .is_err()
        );
        assert!(
            thread_action_command(
                &idle,
                &ThreadActionRequest {
                    action_id: "session.discard".into(),
                    ..request.clone()
                }
            )
            .is_err()
        );
        assert!(
            thread_action_command(
                &idle,
                &ThreadActionRequest {
                    action_id: "copySessionId".into(),
                    ..request
                }
            )
            .is_err()
        );
        let mut limited = capabilities.clone();
        limited.methods.remove("session.rename");
        let no_rename = thread_actions(&ThreadActionContext {
            capabilities: &limited,
            ..context
        });
        assert!(!no_rename.items.iter().any(|i| i.id == "rename"));
    }
    #[test]
    fn native_fork_title_increment_handles_unicode_brackets_and_unbounded_decimals() {
        assert_eq!(increased_fork_title("Chat"), "Chat (1)");
        assert_eq!(increased_fork_title("对话（0009）"), "对话（10）");
        assert_eq!(
            increased_fork_title("Chat (9999999999999999999999999)"),
            "Chat (10000000000000000000000000)"
        );
        assert_eq!(increased_fork_title("Chat (x)"), "Chat (x) (1)");
    }
}
