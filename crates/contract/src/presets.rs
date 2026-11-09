//! Structured Kernel session openings. Execution authority is supplied by the host.
use crate::*;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionPresetSelection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp: Option<Vec<SessionPresetMcpReference>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<String>>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionPresetDefaults {
    pub provider: String,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionPresetMcpReference {
    pub server: String,
    pub tools: Vec<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionPreset {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layers: Option<Vec<SessionPresetLayer>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp: Option<Vec<SessionPresetMcpReference>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defaults: Option<SessionPresetDefaults>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum SessionPresetLayer {
    Kernel {
        id: String,
    },
    Inline {
        id: String,
        version: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        identity: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        duty: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tools: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        mcp: Option<Vec<SessionPresetMcpReference>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        skills: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        defaults: Option<Box<SessionPresetDefaults>>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPresetDescription {
    pub format: u64,
    pub render_version: u64,
    pub bases: Vec<SessionPresetBase>,
    pub kernel_layers: Vec<SessionPresetKernelLayer>,
    pub limits: SessionPresetLimits,
    pub invalid_reasons: Vec<String>,
    #[serde(default)]
    pub selectable_tools: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionPresetBase {
    pub id: String,
    pub identity: Option<String>,
    pub rules: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionPresetKernelLayer {
    pub kind: String,
    pub id: String,
    pub version: String,
    pub title: Option<String>,
    pub identity: Option<String>,
    pub duty: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPresetLimits {
    pub max_layers: usize,
    pub max_id_bytes: usize,
    pub max_version_bytes: usize,
    pub max_identity_bytes: usize,
    pub max_duty_bytes: usize,
    pub max_text_bytes: usize,
    pub max_tools: usize,
    pub max_mcp_servers: usize,
    pub max_mcp_tools_per_server: usize,
    pub max_skills: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPresetResolution {
    pub base: String,
    pub layers: Vec<SessionPresetLayerSummary>,
    pub preset_text: String,
    pub digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp: Option<Vec<SessionPresetResolvedMcp>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defaults: Option<SessionPresetDefaults>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_readiness: Option<Vec<SessionPresetMcpReadiness>>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionPresetLayerSummary {
    pub kind: String,
    pub id: String,
    pub version: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionPresetResolvedMcp {
    pub server: String,
    pub digest: String,
    pub tools: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPresetMcpReadiness {
    pub server: String,
    pub status: String,
    pub code: Option<String>,
    pub missing_tools: Option<Vec<String>>,
    pub refused_tools: Option<Vec<SessionPresetRefusedTool>>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionPresetRefusedTool {
    pub tool: String,
    pub code: String,
}

/// Mirrors the public Kernel wire value. Only the authenticated embedding host supplies it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionExecutionConfiguration {
    pub cwd: String,
    #[serde(default)]
    pub read_roots: Vec<String>,
    #[serde(default)]
    pub writable_roots: Vec<String>,
    #[serde(default)]
    pub toolchain_roots: Vec<String>,
    #[serde(default)]
    pub network: bool,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_wall_seconds: Option<u64>,
    #[serde(default)]
    pub mcp: Vec<SessionExecutionMcpConnection>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionExecutionMcpConnection {
    pub name: String,
    pub transport: SessionExecutionMcpTransport,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum SessionExecutionMcpTransport {
    Http {
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        credential_id: Option<String>,
    },
    Stdio {
        channel: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionPresetPreviewRequest {
    pub execution: SessionExecutionConfiguration,
    pub preset: SessionPreset,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativePresetCreateRequest {
    pub execution: SessionExecutionConfiguration,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset: Option<SessionPreset>,
}
impl NativePresetCreateRequest {
    pub fn validate(&self) -> Result<()> {
        require(
            self.identity_profile.is_none() || self.preset.is_none(),
            "preset and identityProfile are exclusive",
        )?;
        self.execution.validate()
    }
}
impl SessionExecutionConfiguration {
    pub fn validate(&self) -> Result<()> {
        require(
            std::iter::once(&self.cwd)
                .chain(&self.read_roots)
                .chain(&self.writable_roots)
                .chain(&self.toolchain_roots)
                .all(|p| p.starts_with('/') && !p.contains('\0')),
            "execution paths must be absolute",
        )?;
        require(
            self.max_wall_seconds
                .is_none_or(|n| n > 0 && n <= 9_007_199_254_740_991),
            "execution wall limit is invalid",
        )?;
        let mut names = BTreeSet::new();
        for connection in &self.mcp {
            require(
                !connection.name.is_empty() && names.insert(&connection.name),
                "execution MCP connection names must be unique",
            )?;
        }
        Ok(())
    }
}
impl SessionPreset {
    #[must_use]
    pub fn has_defaults(&self) -> bool {
        self.defaults.is_some()
            || self.layers.as_ref().is_some_and(|layers| {
                layers.iter().any(|layer| {
                    matches!(
                        layer,
                        SessionPresetLayer::Inline {
                            defaults: Some(_),
                            ..
                        }
                    )
                })
            })
    }
    pub fn validate_description(&self, description: &SessionPresetDescription) -> Result<()> {
        let layers = self.layers.as_deref().unwrap_or_default();
        let limits = &description.limits;
        require(layers.len() <= limits.max_layers, "too many preset layers")?;
        let mut ids = BTreeSet::new();
        let mut identities = 0;
        validate_selection(
            self.tools.as_deref(),
            self.mcp.as_deref(),
            self.skills.as_deref(),
            description,
        )?;
        for layer in layers {
            match layer {
                SessionPresetLayer::Kernel { id } => {
                    require(
                        description
                            .kernel_layers
                            .iter()
                            .any(|layer| &layer.id == id),
                        "unknown Kernel preset layer",
                    )?;
                    require(ids.insert(id), "duplicate preset layer")?;
                }
                SessionPresetLayer::Inline {
                    id,
                    version,
                    identity,
                    duty,
                    tools,
                    mcp,
                    skills,
                    ..
                } => {
                    require(ids.insert(id), "duplicate preset layer")?;
                    require(
                        !id.is_empty()
                            && id.len() <= limits.max_id_bytes
                            && id.as_bytes()[0].is_ascii_alphanumeric()
                            && id
                                .bytes()
                                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
                        "invalid inline preset ID",
                    )?;
                    require(
                        !version.is_empty()
                            && version.len() <= limits.max_version_bytes
                            && version.bytes().all(|b| (0x21..=0x7e).contains(&b)),
                        "invalid inline preset version",
                    )?;
                    if let Some(identity) = identity {
                        identities += 1;
                        require(
                            !identity.trim().is_empty()
                                && !identity.contains(['\n', '\r'])
                                && identity.len() <= limits.max_identity_bytes,
                            "invalid inline preset identity",
                        )?;
                    }
                    require(
                        duty.as_ref()
                            .is_none_or(|text| text.len() <= limits.max_duty_bytes),
                        "inline preset duty is too large",
                    )?;
                    validate_selection(
                        tools.as_deref(),
                        mcp.as_deref(),
                        skills.as_deref(),
                        description,
                    )?;
                }
            }
        }
        require(identities <= 1, "multiple inline preset identities")
    }
}
fn validate_selection(
    tools: Option<&[String]>,
    mcp: Option<&[SessionPresetMcpReference]>,
    skills: Option<&[String]>,
    description: &SessionPresetDescription,
) -> Result<()> {
    let limits = &description.limits;
    if let Some(tools) = tools {
        require(
            tools.len() <= limits.max_tools
                && tools
                    .iter()
                    .all(|tool| description.selectable_tools.contains(tool)),
            "unknown or excessive preset tools",
        )?;
    }
    if let Some(skills) = skills {
        require(
            skills.len() <= limits.max_skills && skills.iter().all(|name| !name.is_empty()),
            "invalid preset skills",
        )?;
    }
    if let Some(mcp) = mcp {
        require(
            mcp.len() <= limits.max_mcp_servers
                && mcp.iter().all(|reference| {
                    !reference.server.is_empty()
                        && reference.tools.len() <= limits.max_mcp_tools_per_server
                        && reference.tools.iter().all(|tool| !tool.is_empty())
                }),
            "invalid preset MCP references",
        )?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn structured_presets_preserve_defaults_and_refuse_secret_or_unknown_members() {
        let preset:SessionPreset=serde_json::from_value(serde_json::json!({"layers":[{"kind":"inline","id":"role","version":"1","duty":"Do work","defaults":{"provider":"configured","model":"flash","reasoningEffort":"low"}}],"tools":["read"]})).unwrap();
        assert!(preset.has_defaults());
        assert_eq!(
            serde_json::to_value(&preset).unwrap()["layers"][0]["defaults"]["reasoningEffort"],
            "low"
        );
        assert!(
            serde_json::from_value::<SessionPreset>(
                serde_json::json!({"mcp":[{"server":"host","tools":["read"],"apiKey":"secret"}]})
            )
            .is_err()
        );
        assert!(serde_json::from_value::<SessionPreset>(serde_json::json!({"layers":[{"kind":"inline","id":"x","version":"1","credentials":{}}]})).is_err());
        assert_eq!(
            serde_json::to_value(SessionPreset::default()).unwrap(),
            serde_json::json!({})
        );
    }
    #[test]
    fn native_creation_never_combines_identity_sugar_and_structured_preset() {
        let request:NativePresetCreateRequest=serde_json::from_value(serde_json::json!({"execution":{"cwd":"/project"},"identityProfile":"general","preset":{}})).unwrap();
        assert!(request.validate().is_err());
        assert!(
            serde_json::from_value::<NativePresetCreateRequest>(
                serde_json::json!({"execution":{"cwd":"/project"},"agentPreset":"coding"})
            )
            .is_err()
        );
    }
}
