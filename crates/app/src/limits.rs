use std::time::Duration;

/// Budgets applied to a single Run (time, tokens, tool calls).
#[derive(Debug, Clone, Default)]
pub struct RunBudgets {
    pub max_duration: Option<Duration>,
    pub max_tokens: Option<u64>,
    pub max_tool_calls: Option<u64>,
}

impl RunBudgets {
    #[must_use]
    pub fn unlimited() -> Self {
        Self::default()
    }

    /// Cap child budgets so they cannot exceed parent authority.
    #[must_use]
    pub fn carve_for_child(&self, requested_tool_calls: Option<u64>) -> Self {
        let parent_tools = self.max_tool_calls.unwrap_or(8);
        let tools = requested_tool_calls.unwrap_or(4).min(parent_tools).max(1);
        Self {
            max_duration: self.max_duration.map(|d| d / 2),
            max_tokens: self.max_tokens.map(|t| (t / 2).max(1)),
            max_tool_calls: Some(tools),
        }
    }
}

/// Worker-wide concurrency and default Run budgets.
#[derive(Debug, Clone)]
pub struct RunLimits {
    /// Maximum concurrent Active Runs across all Sessions.
    pub global_active_cap: usize,
    pub default_budgets: RunBudgets,
}

impl Default for RunLimits {
    fn default() -> Self {
        Self {
            global_active_cap: 8,
            default_budgets: RunBudgets::unlimited(),
        }
    }
}

impl RunLimits {
    #[must_use]
    pub fn with_global_cap(mut self, cap: usize) -> Self {
        self.global_active_cap = cap.max(1);
        self
    }

    #[must_use]
    pub fn with_budgets(mut self, budgets: RunBudgets) -> Self {
        self.default_budgets = budgets;
        self
    }
}
