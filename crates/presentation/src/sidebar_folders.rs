//! Display membership is separate from SessionAddress and workspace routing.
use serde::{Deserialize, Serialize};
use tekes_agent_contract::{Result, require};
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum SidebarFolderIdentity {
    Id(String),
    Path(String),
    None,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarFolderKey {
    pub endpoint_id: String,
    pub identity: SidebarFolderIdentity,
}
impl SidebarFolderKey {
    pub fn validate(&self) -> Result<()> {
        require(
            !self.endpoint_id.is_empty(),
            "Sidebar folder endpoint is required",
        )?;
        require(
            match &self.identity {
                SidebarFolderIdentity::Id(value) | SidebarFolderIdentity::Path(value) => {
                    !value.is_empty()
                }
                SidebarFolderIdentity::None => true,
            },
            "empty Sidebar folder identity",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn folder_identity_is_endpoint_scoped_and_never_collides_with_path_or_none() {
        let id = SidebarFolderKey {
            endpoint_id: "pi".into(),
            identity: SidebarFolderIdentity::Id("path:/work".into()),
        };
        let path = SidebarFolderKey {
            endpoint_id: "pi".into(),
            identity: SidebarFolderIdentity::Path("/work".into()),
        };
        let other = SidebarFolderKey {
            endpoint_id: "dsh".into(),
            identity: path.identity.clone(),
        };
        assert_ne!(id, path);
        assert_ne!(path, other);
        for value in [id, path, other] {
            value.validate().unwrap();
            assert_eq!(
                serde_json::from_str::<SidebarFolderKey>(&serde_json::to_string(&value).unwrap())
                    .unwrap(),
                value
            );
        }
    }
    #[test]
    fn malformed_and_duplicate_folder_preferences_are_rejected() {
        let mut key = SidebarFolderKey {
            endpoint_id: "pi".into(),
            identity: SidebarFolderIdentity::Path(String::new()),
        };
        assert!(key.validate().is_err());
        key.identity = SidebarFolderIdentity::None;
        key.validate().unwrap();
        let value = crate::SidebarPreferences {
            collapsed_folders: vec![key.clone(), key],
            ..Default::default()
        };
        assert!(value.validate().is_err());
    }
}
