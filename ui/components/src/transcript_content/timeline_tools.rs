//! One visible continuous run across original message/control ownership.
use super::*;

pub(super) fn flatten(node: &mut Content, value: bool) {
    if let Some(group) = node.component_mut::<ToolRun>() {
        group.set_flattened(value);
    } else if let Some(fragment) = node.component_mut::<OutputMessageFragment>() {
        fragment.set_tools_flattened(value);
    }
}

impl TranscriptContent {
    pub(super) fn next_group_revision(&self) -> Result<u64, &'static str> {
        self.group_revision
            .checked_add(1)
            .ok_or("timeline group display revision exhausted")
    }
    pub(super) fn original_rows(&self) -> Vec<(&TranscriptContentRowKey, &Content)> {
        let mut rows = vec![];
        for (key, node) in self
            .keys
            .iter()
            .zip(&self.rows.children)
            .chain(self.hidden_rows.iter())
        {
            if let Some(members) = self.group_members.get(key) {
                rows.extend(
                    members
                        .iter()
                        .map(|(key, _)| key)
                        .zip(node.component::<ToolRun>().unwrap().member_nodes()),
                );
            } else {
                rows.push((key, node));
            }
        }
        rows
    }
    pub(super) fn check_group_members(&self) -> Result<(), &'static str> {
        for (key, members) in &self.group_members {
            let node = self
                .keys
                .iter()
                .position(|candidate| candidate == key)
                .map(|index| &self.rows.children[index])
                .or_else(|| self.hidden_rows.get(key))
                .ok_or("missing original timeline group")?;
            let actual: Vec<_> = node
                .component::<ToolRun>()
                .ok_or("timeline group role changed")?
                .member_nodes()
                .map(Content::id)
                .collect();
            if actual != members.iter().map(|(_, id)| *id).collect::<Vec<_>>() {
                return Err("missing or replaced original timeline members");
            }
        }
        Ok(())
    }
    fn reading_group_for(&self, key: &TranscriptContentRowKey) -> Option<ComponentId> {
        self.group_members
            .iter()
            .find(|(_, members)| members.iter().any(|(member, _)| member == key))
            .and_then(|(group, _)| self.keys.iter().position(|candidate| candidate == group))
            .map(|index| self.rows.children[index].id())
    }
    /// Original controls retained behind the current visible reading group.
    /// These are not reading-item IDs and can be unmounted while it is folded.
    pub fn message_tool_fragment(
        &self,
        owner: &TranscriptMessageIdentity,
        tool: &OutputMessageToolFragmentKey,
    ) -> Option<&OutputMessageFragment> {
        self.original_rows().into_iter().find(|(key, _)| matches!(key, TranscriptContentRowKey::MessageToolFragment(candidate, key) if candidate == owner && key == tool)).and_then(|(_, node)| node.component::<OutputMessageFragment>())
    }
    pub fn message_tool_fragment_ids(
        &self,
        owner: &TranscriptMessageIdentity,
    ) -> Vec<(OutputMessageToolFragmentKey, ComponentId)> {
        self.original_rows()
            .into_iter()
            .filter_map(|(key, node)| {
                if let TranscriptContentRowKey::MessageToolFragment(candidate, tool) = key {
                    (candidate == owner && self.row_id(key).is_some())
                        .then(|| (tool.clone(), node.id()))
                } else {
                    None
                }
            })
            .collect()
    }
    pub(super) fn original_reading_row(
        &self,
        key: &TranscriptContentRowKey,
    ) -> Option<ComponentId> {
        self.reading_group_for(key)
    }
    pub(super) fn group_timeline_tools<'a>(
        &self,
        mut rows: Vec<Planned<'a>>,
        snapshot: &TranscriptSnapshot,
        document: &'a TranscriptDocument<'a>,
    ) -> Result<Vec<Planned<'a>>, &'static str> {
        let calls: BTreeMap<_, _> = document
            .sections
            .iter()
            .flat_map(|section| {
                section
                    .tools
                    .iter()
                    .map(move |tool| ((section.id, tool.id.as_str()), *tool))
            })
            .collect();
        let candidates: Vec<Option<Vec<&TranscriptToolCall>>> = rows
            .iter()
            .map(|row| {
                let (turn, ids) = match &row.content {
                    PlannedContent::Tools(tools)
                        if tools
                            .iter()
                            .all(|(_, placement)| *placement == ToolActivityPlacement::Call) =>
                    {
                        (
                            tools[0].0.turn_id.as_str(),
                            tools
                                .iter()
                                .map(|(tool, _)| tool.id.as_str())
                                .collect::<Vec<_>>(),
                        )
                    }
                    PlannedContent::ToolFragment(_, key, turn, ids)
                        if key.block_index.is_none() =>
                    {
                        (*turn, ids.iter().map(String::as_str).collect())
                    }
                    _ => return None,
                };
                ids.into_iter()
                    .map(|id| {
                        let tool = calls.get(&(turn, id)).copied()?;
                        (tool.source_position.is_some()
                            || snapshot.thinking_sources.by_id.contains_key(id))
                        .then_some(tool)
                    })
                    .collect::<Option<Vec<_>>>()
            })
            .collect();
        let mut counts = BTreeMap::new();
        for tool in candidates.iter().flatten().flatten() {
            *counts
                .entry((tool.turn_id.as_str(), tool.id.as_str()))
                .or_insert(0) += 1;
        }
        let mut runs: Vec<Vec<usize>> = vec![];
        let mut pending = vec![];
        for (index, row) in rows.iter().enumerate() {
            if row.hidden {
                continue;
            }
            let eligible = candidates[index].as_ref().filter(|tools| {
                !tools.is_empty()
                    && tools
                        .iter()
                        .all(|tool| counts[&(tool.turn_id.as_str(), tool.id.as_str())] == 1)
            });
            if let Some(tools) = eligible {
                if pending.first().is_some_and(|first: &usize| {
                    candidates[*first].as_ref().unwrap()[0].turn_id != tools[0].turn_id
                }) {
                    runs.push(std::mem::take(&mut pending));
                }
                pending.push(index);
            } else if !pending.is_empty() {
                runs.push(std::mem::take(&mut pending));
            }
        }
        if !pending.is_empty() {
            runs.push(pending);
        }
        let mut starts = BTreeMap::new();
        for run in runs {
            let tools: Vec<_> = run
                .iter()
                .flat_map(|index| candidates[*index].as_ref().unwrap().iter().copied())
                .collect();
            self.labels.tool_run.summary(&self.scope, &tools)?;
            let key = TranscriptContentRowKey::ContinuousTools {
                turn_id: tools[0].turn_id.clone(),
                first_tool_id: tools[0].id.clone(),
            };
            let members = run.iter().map(|index| rows[*index].key.clone()).collect();
            for index in &run {
                rows[*index].hidden = true;
                rows[*index].group_owner = Some(key.clone());
            }
            starts.insert(
                run[0],
                Planned {
                    key,
                    alias: None,
                    thinking: None,
                    narration_frame: None,
                    hidden: false,
                    group_owner: None,
                    activity: Some(tools[0].turn_id.as_str()),
                    content: PlannedContent::ContinuousTools { members, tools },
                },
            );
        }
        let mut result = vec![];
        for (index, row) in rows.into_iter().enumerate() {
            if let Some(group) = starts.remove(&index) {
                result.push(group);
            }
            result.push(row);
        }
        Ok(result)
    }
}
