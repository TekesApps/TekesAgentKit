//! V2 `ProcessComponentOptions` / `Capabilities` compatibility descriptors for
//! isolating heavy worker panes into child processes when desired by a host.

use tekes_ui_app_host::{
    ContainerOptions, ContainerToolbarHost, ContainerToolbarPosition, ProcessComponentOptions,
};
use tekes_ui_app_protocol::component::Capabilities;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AgentProcessComponentDescriptor {
    pub id: &'static str,
    pub title: &'static str,
    pub switcher_icon: &'static str,
    pub list_height: Option<f32>,
    pub capabilities: Capabilities,
}

pub const CORE_PROCESS_COMPONENTS: [AgentProcessComponentDescriptor; 3] = [
    AgentProcessComponentDescriptor {
        id: "agent-run",
        title: "Agent 对话",
        switcher_icon: "tekes-side-chat",
        list_height: None,
        capabilities: Capabilities {
            services: true,
            toolbar: true,
            view: true,
        },
    },
    AgentProcessComponentDescriptor {
        id: "agent-sidebar",
        title: "会话导航",
        switcher_icon: "tekes-side-bar",
        list_height: None,
        capabilities: Capabilities {
            services: true,
            toolbar: true,
            view: true,
        },
    },
    AgentProcessComponentDescriptor {
        id: "agent-summary",
        title: "运行摘要",
        switcher_icon: "tekes-tab-summary",
        list_height: Some(280.0),
        capabilities: Capabilities {
            services: true,
            toolbar: true,
            view: true,
        },
    },
];

impl AgentProcessComponentDescriptor {
    pub fn container_options(self) -> ContainerOptions {
        let mut opts = ContainerOptions {
            title: Some(self.title.to_owned()),
            switcher_icon: Some(self.switcher_icon.to_owned()),
            toolbar_host: ContainerToolbarHost::Container {
                position: ContainerToolbarPosition::Top,
            },
            list_height: self.list_height,
            ..ContainerOptions::default()
        };
        opts.content_padding = Some(0.0);
        opts
    }

    pub fn process_component_options(self) -> ProcessComponentOptions {
        ProcessComponentOptions::new(self.id, 1)
    }
}
