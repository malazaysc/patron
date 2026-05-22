#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskState {
    Draft,
    ReadyForPlanning,
    Planning,
    ReadyForDevelopment,
    Developing,
    ReadyForReview,
    Reviewing,
    ReadyForQa,
    QaRunning,
    FixRequired,
    ReadyForPr,
    PrPrepared,
    AwaitingHuman,
    Done,
    Blocked,
    Failed,
    Cancelled,
}

#[allow(dead_code)]
impl TaskState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::ReadyForPlanning => "ready_for_planning",
            Self::Planning => "planning",
            Self::ReadyForDevelopment => "ready_for_development",
            Self::Developing => "developing",
            Self::ReadyForReview => "ready_for_review",
            Self::Reviewing => "reviewing",
            Self::ReadyForQa => "ready_for_qa",
            Self::QaRunning => "qa_running",
            Self::FixRequired => "fix_required",
            Self::ReadyForPr => "ready_for_pr",
            Self::PrPrepared => "pr_prepared",
            Self::AwaitingHuman => "awaiting_human",
            Self::Done => "done",
            Self::Blocked => "blocked",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn is_ready_state(self) -> bool {
        matches!(
            self,
            Self::ReadyForPlanning
                | Self::ReadyForDevelopment
                | Self::ReadyForReview
                | Self::ReadyForQa
                | Self::ReadyForPr
        )
    }

    pub fn is_active_stage(self) -> bool {
        matches!(
            self,
            Self::Planning | Self::Developing | Self::Reviewing | Self::QaRunning
        )
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActorKind {
    Human,
    System,
    Runner,
    Orchestrator,
}

#[allow(dead_code)]
impl ActorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::System => "system",
            Self::Runner => "runner",
            Self::Orchestrator => "orchestrator",
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HumanAction {
    ApproveTask,
    ApprovePlan,
    ReviewPr,
    ResolveBlock,
}

#[allow(dead_code)]
impl HumanAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ApproveTask => "approve_task",
            Self::ApprovePlan => "approve_plan",
            Self::ReviewPr => "review_pr",
            Self::ResolveBlock => "resolve_block",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub struct TransitionMetadata {
    pub actor: ActorKind,
    pub actor_id: Option<String>,
    pub occurred_at: String,
    pub reason_code: Option<String>,
    pub reason_text: String,
    pub run_id: Option<String>,
    pub required_human_action: Option<HumanAction>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub enum TransitionError {
    MissingOccurredAt,
    MissingReasonText,
    MissingRunId,
    MissingBlockedReasonCode,
    MissingHumanAction,
    InvalidTransition { from: TaskState, to: TaskState },
}

#[allow(dead_code)]
pub struct TaskStateMachine;

#[allow(dead_code)]
impl TaskStateMachine {
    pub fn validate_transition(
        from: TaskState,
        to: TaskState,
        metadata: &TransitionMetadata,
    ) -> Result<(), TransitionError> {
        if metadata.occurred_at.trim().is_empty() {
            return Err(TransitionError::MissingOccurredAt);
        }

        if metadata.reason_text.trim().is_empty() {
            return Err(TransitionError::MissingReasonText);
        }

        if requires_run_id(from, to) && metadata.run_id.is_none() {
            return Err(TransitionError::MissingRunId);
        }

        if to == TaskState::Blocked
            && metadata
                .reason_code
                .as_deref()
                .is_none_or(|reason_code| reason_code.trim().is_empty())
        {
            return Err(TransitionError::MissingBlockedReasonCode);
        }

        if to == TaskState::AwaitingHuman && metadata.required_human_action.is_none() {
            return Err(TransitionError::MissingHumanAction);
        }

        if is_allowed_transition(from, to) {
            Ok(())
        } else {
            Err(TransitionError::InvalidTransition { from, to })
        }
    }
}

#[allow(dead_code)]
fn requires_run_id(from: TaskState, to: TaskState) -> bool {
    from.is_active_stage() || to.is_active_stage()
}

#[allow(dead_code)]
fn is_allowed_transition(from: TaskState, to: TaskState) -> bool {
    if matches!(
        to,
        TaskState::Failed | TaskState::Cancelled | TaskState::Blocked
    ) {
        return from != to;
    }

    if from == TaskState::Blocked {
        return to.is_ready_state();
    }

    matches!(
        (from, to),
        (TaskState::Draft, TaskState::ReadyForPlanning)
            | (TaskState::ReadyForPlanning, TaskState::Planning)
            | (TaskState::Planning, TaskState::ReadyForDevelopment)
            | (TaskState::ReadyForDevelopment, TaskState::Developing)
            | (TaskState::Developing, TaskState::ReadyForReview)
            | (TaskState::ReadyForReview, TaskState::Reviewing)
            | (TaskState::Reviewing, TaskState::ReadyForQa)
            | (TaskState::Reviewing, TaskState::FixRequired)
            | (TaskState::ReadyForQa, TaskState::QaRunning)
            | (TaskState::QaRunning, TaskState::ReadyForPr)
            | (TaskState::QaRunning, TaskState::FixRequired)
            | (TaskState::FixRequired, TaskState::ReadyForDevelopment)
            | (TaskState::ReadyForPr, TaskState::PrPrepared)
            | (TaskState::PrPrepared, TaskState::AwaitingHuman)
            | (TaskState::AwaitingHuman, TaskState::Done)
    )
}

#[cfg(test)]
mod tests {
    use super::{
        ActorKind, HumanAction, TaskState, TaskStateMachine, TransitionError, TransitionMetadata,
    };

    fn metadata() -> TransitionMetadata {
        TransitionMetadata {
            actor: ActorKind::System,
            actor_id: None,
            occurred_at: "2026-05-22T12:00:00Z".into(),
            reason_code: Some("normal_progression".into()),
            reason_text: "advance pipeline".into(),
            run_id: Some("run-001".into()),
            required_human_action: None,
        }
    }

    #[test]
    fn accepts_documented_transition() {
        let metadata = metadata();

        let result = TaskStateMachine::validate_transition(
            TaskState::ReadyForPlanning,
            TaskState::Planning,
            &metadata,
        );

        assert_eq!(result, Ok(()));
    }

    #[test]
    fn rejects_invalid_transition() {
        let metadata = metadata();

        let result = TaskStateMachine::validate_transition(
            TaskState::Draft,
            TaskState::Developing,
            &metadata,
        );

        assert_eq!(
            result,
            Err(TransitionError::InvalidTransition {
                from: TaskState::Draft,
                to: TaskState::Developing,
            })
        );
    }

    #[test]
    fn requires_run_id_for_active_stage_transitions() {
        let mut metadata = metadata();
        metadata.run_id = None;

        let result = TaskStateMachine::validate_transition(
            TaskState::ReadyForPlanning,
            TaskState::Planning,
            &metadata,
        );

        assert_eq!(result, Err(TransitionError::MissingRunId));
    }

    #[test]
    fn requires_block_reason_code_for_blocked_transitions() {
        let mut metadata = metadata();
        metadata.reason_code = None;

        let result = TaskStateMachine::validate_transition(
            TaskState::ReadyForQa,
            TaskState::Blocked,
            &metadata,
        );

        assert_eq!(result, Err(TransitionError::MissingBlockedReasonCode));
    }

    #[test]
    fn requires_human_action_for_awaiting_human() {
        let mut metadata = metadata();
        metadata.run_id = None;

        let result = TaskStateMachine::validate_transition(
            TaskState::PrPrepared,
            TaskState::AwaitingHuman,
            &metadata,
        );

        assert_eq!(result, Err(TransitionError::MissingHumanAction));
    }

    #[test]
    fn allows_blocked_to_ready_state_recovery() {
        let mut metadata = metadata();
        metadata.run_id = None;
        metadata.reason_code = Some("human_unblocked".into());
        metadata.reason_text = "operator resolved the block".into();

        let result = TaskStateMachine::validate_transition(
            TaskState::Blocked,
            TaskState::ReadyForDevelopment,
            &metadata,
        );

        assert_eq!(result, Ok(()));
    }

    #[test]
    fn awaiting_human_can_progress_to_done() {
        let mut metadata = metadata();
        metadata.run_id = None;
        metadata.required_human_action = Some(HumanAction::ReviewPr);

        let result = TaskStateMachine::validate_transition(
            TaskState::PrPrepared,
            TaskState::AwaitingHuman,
            &metadata,
        );

        assert_eq!(result, Ok(()));

        let done_result = TaskStateMachine::validate_transition(
            TaskState::AwaitingHuman,
            TaskState::Done,
            &metadata,
        );

        assert_eq!(done_result, Ok(()));
    }
}
