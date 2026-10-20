//! Presentation facts only: never infer success from an idle run or estimate token counts.
use tekes_agent_presentation::{ChildComposerAvailability, ConnectionPhase, RunPhase, RunSnapshot};
use tekes_ui_controls::{ButtonTextRenderer, composition::*, layout::MeasuredChild};
use tekes_ui_scene::{Rect, Size};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutputUsage {
    Live { output_tokens: u64 },
    Reported { output_tokens: u64, complete: bool },
}
impl OutputUsage {
    pub fn read_turn(turn: &tekes_agent_presentation::TranscriptTurn) -> Option<Self> {
        if turn.status == "running" {
            turn.live_usage.as_ref().map(|usage| Self::Live {
                output_tokens: usage.output_tokens,
            })
        } else {
            turn.reported_usage.as_ref().map(|usage| Self::Reported {
                output_tokens: usage.output_tokens,
                complete: usage.is_complete,
            })
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct IndicatorFacts {
    pub connection: ConnectionPhase,
    pub run: Option<RunPhase>,
    pub history_stale: bool,
    pub connection_error: Option<String>,
    pub run_error: Option<String>,
    pub waiting: usize,
    pub queued: usize,
    pub output_usage: Option<OutputUsage>,
    pub child_availability: Option<ChildComposerAvailability>,
}
impl IndicatorFacts {
    pub fn read(snapshot: &RunSnapshot) -> Self {
        let turn = snapshot.transcript.turns.last();
        let output_usage = turn.and_then(OutputUsage::read_turn);
        Self {
            connection: snapshot.connection.phase.clone(),
            run: snapshot.run.as_ref().map(|run| run.phase.clone()),
            history_stale: snapshot.transcript.is_stale || snapshot.connection.history_is_stale,
            connection_error: snapshot.connection.message.clone(),
            run_error: snapshot.run.as_ref().and_then(|run| run.error.clone()),
            waiting: snapshot.run.as_ref().map_or(0, |run| run.actionables.len()),
            queued: snapshot.run.as_ref().map_or(0, |run| run.queue.len()),
            output_usage,
            child_availability: snapshot.child_availability.clone(),
        }
    }
    pub fn accessible_text(&self) -> String {
        let connection = connection_status_text(&self.connection);
        let run = match self.run {
            Some(RunPhase::Running) => "执行中",
            Some(RunPhase::WaitingForInput) => "等待输入",
            Some(RunPhase::Loading) => "正在加载",
            Some(RunPhase::Idle) => "空闲",
            Some(RunPhase::Interrupted) => "已中断",
            Some(RunPhase::Archived) => "已归档",
            Some(RunPhase::Unavailable) => "暂不可用",
            None => "未选择会话",
        };
        let mut parts = vec![connection.to_owned(), run.to_owned()];
        if let Some(child) = &self.child_availability {
            parts.push(
                match child {
                    ChildComposerAvailability::Loading => "子代理控制能力未确认",
                    ChildComposerAvailability::Available => "子代理可继续执行",
                    ChildComposerAvailability::OneShot => "子代理为单次执行，历史可读",
                    ChildComposerAvailability::ParentUnavailable => {
                        "父代理暂不可用，子代理历史可读"
                    }
                    ChildComposerAvailability::Unavailable => "子代理控制不可用，历史可读",
                }
                .into(),
            );
        }
        if self.history_stale {
            parts.push("历史待同步".into());
        }
        if self.waiting > 0 {
            parts.push(format!("{} 项等待处理", self.waiting));
        }
        if self.queued > 0 {
            parts.push(format!("{} 项排队", self.queued));
        }
        if let Some(usage) = &self.output_usage {
            parts.push(match usage {
                OutputUsage::Live { output_tokens } => {
                    format!("输出 {output_tokens} tokens · 实时报告")
                }
                OutputUsage::Reported {
                    output_tokens,
                    complete,
                } => format!(
                    "输出 {output_tokens} tokens{}",
                    if *complete { "" } else { " · 部分报告" }
                ),
            });
        }
        parts.extend(self.connection_error.iter().cloned());
        parts.extend(self.run_error.iter().cloned());
        parts.join(" · ")
    }
}

/// Shared default copy for a canonical connection phase; products may format facts.
pub fn connection_status_text(phase: &ConnectionPhase) -> &'static str {
    match phase {
        ConnectionPhase::Connecting => "正在连接",
        ConnectionPhase::Connected => "已连接",
        ConnectionPhase::Reconnecting => "正在重连",
        ConnectionPhase::Degraded => "连接受限",
        ConnectionPhase::Failed => "连接失败",
        ConnectionPhase::Stopped => "已停止连接",
    }
}

/// Independently mountable accessible status. Host chooses placement; no timer or reconnect loop.
#[derive(Debug)]
pub struct Indicators {
    text: TextContent,
    facts: Option<IndicatorFacts>,
    revision: u64,
}
impl Default for Indicators {
    fn default() -> Self {
        Self {
            text: TextContent::new("未连接"),
            facts: None,
            revision: 0,
        }
    }
}
impl Indicators {
    pub fn facts(&self) -> Option<&IndicatorFacts> {
        self.facts.as_ref()
    }
    pub fn update(&mut self, snapshot: &RunSnapshot) -> Result<(), &'static str> {
        self.update_with(snapshot, IndicatorFacts::accessible_text)
    }
    /// Localize/product-format canonical facts without creating another status machine.
    pub fn update_with(
        &mut self,
        snapshot: &RunSnapshot,
        format: impl FnOnce(&IndicatorFacts) -> String,
    ) -> Result<(), &'static str> {
        if snapshot.revision == 0 || snapshot.revision < self.revision {
            return Err("stale or unversioned indicator input");
        }
        let facts = IndicatorFacts::read(snapshot);
        self.text.text = format(&facts);
        self.facts = Some(facts);
        self.revision = snapshot.revision;
        Ok(())
    }
}
impl Component for Indicators {
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        self.text.measure(text, available)
    }
    fn paint(
        &mut self,
        scene: &mut tekes_ui_scene::Scene,
        text: &mut dyn ButtonTextRenderer,
        frame: Rect,
        clip: Rect,
        theme: ContentTheme,
    ) {
        self.text.paint(scene, text, frame, clip, theme);
    }
    fn semantics(&self) -> ContentSemantics {
        self.text.semantics()
    }
}
