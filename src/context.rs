//! Provider-independent context capacity, budgeting, and prompt planning.
//!
//! A configured profile limit is a git-explain budgeting cap. It never changes
//! the model server's allocation.

use crate::config::GenerationConfig;

pub const PLAN_VERSION: &str = "context-plan-v1";
const UNKNOWN_CONTEXT_FALLBACK: u32 = 8_192;
const NORMAL_OUTPUT_RESERVE: u32 = 1_024;
const DEEP_OUTPUT_RESERVE: u32 = 2_500;
const MIN_SAFETY_MARGIN: u32 = 384;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextSource {
    RuntimeDetected,
    ProfileConfigured,
    ModelMetadata,
    ConservativeFallback,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContextCapacity {
    pub model_max: Option<u32>,
    pub runtime_allocated: Option<u32>,
    pub profile_limit: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectiveContext {
    pub tokens: u32,
    pub source: ContextSource,
}

impl ContextCapacity {
    /// The smallest known bound wins. Runtime allocation is never exceeded.
    pub fn effective(&self) -> EffectiveContext {
        let mut candidates = Vec::new();
        if let Some(value) = self.runtime_allocated {
            candidates.push((value, ContextSource::RuntimeDetected));
        }
        if let Some(value) = self.profile_limit {
            candidates.push((value, ContextSource::ProfileConfigured));
        }
        if let Some(value) = self.model_max {
            candidates.push((value, ContextSource::ModelMetadata));
        }
        candidates
            .into_iter()
            .min_by_key(|(value, _)| *value)
            .map(|(tokens, source)| EffectiveContext { tokens, source })
            .unwrap_or(EffectiveContext {
                tokens: UNKNOWN_CONTEXT_FALLBACK,
                source: ContextSource::ConservativeFallback,
            })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextBudget {
    pub total: u32,
    pub output_reserve: u32,
    pub safety_margin: u32,
    pub input_budget: u32,
}

impl ContextBudget {
    pub fn for_generation(
        capacity: &ContextCapacity,
        generation: &GenerationConfig,
        deep: bool,
    ) -> Self {
        let total = capacity.effective().tokens;
        let output_reserve = generation.max_tokens.unwrap_or(if deep {
            DEEP_OUTPUT_RESERVE
        } else {
            NORMAL_OUTPUT_RESERVE
        });
        let safety_margin = MIN_SAFETY_MARGIN.max(total / 12);
        Self {
            total,
            output_reserve,
            safety_margin,
            input_budget: total.saturating_sub(output_reserve.saturating_add(safety_margin)),
        }
    }
}

pub trait TokenEstimator: Send + Sync {
    fn estimate(&self, text: &str) -> u32;
}

/// A deterministic, deliberately conservative fallback for arbitrary compatible
/// endpoints. It can be replaced by a provider tokenizer without changing planning.
#[derive(Clone, Copy, Debug, Default)]
pub struct ConservativeTokenEstimator;

impl TokenEstimator for ConservativeTokenEstimator {
    fn estimate(&self, text: &str) -> u32 {
        let chars = text.chars().count() as u32;
        let lines = text.lines().count() as u32;
        chars.div_ceil(3).saturating_add(lines / 8).max(1)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromptPlan {
    pub budget: ContextBudget,
    pub estimated_input: u32,
}

impl PromptPlan {
    pub fn check(
        prompt: &str,
        budget: ContextBudget,
        estimator: &dyn TokenEstimator,
    ) -> anyhow::Result<Self> {
        let estimated_input = estimator.estimate(prompt);
        if estimated_input > budget.input_budget {
            anyhow::bail!(
                "context budget exceeded before inference: estimated input {estimated_input} tokens, input budget {} tokens (context {}, output reserve {}, safety margin {}). Reduce the changed unit or increase the model server context allocation.",
                budget.input_budget, budget.total, budget.output_reserve, budget.safety_margin
            );
        }
        Ok(Self {
            budget,
            estimated_input,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generation(max_tokens: Option<u32>) -> GenerationConfig {
        GenerationConfig {
            reasoning: None,
            max_tokens,
            temperature: None,
        }
    }

    #[test]
    fn runtime_allocation_always_bounds_model_and_profile_limits() {
        let capacity = ContextCapacity {
            model_max: Some(131_072),
            runtime_allocated: Some(4_096),
            profile_limit: Some(32_768),
        };
        assert_eq!(capacity.effective().tokens, 4_096);
        assert_eq!(capacity.effective().source, ContextSource::RuntimeDetected);
    }

    #[test]
    fn budgets_reserve_more_for_deep_requests_without_underflow() {
        let capacity = ContextCapacity {
            profile_limit: Some(4_096),
            ..Default::default()
        };
        let normal = ContextBudget::for_generation(&capacity, &generation(Some(500)), false);
        let deep = ContextBudget::for_generation(&capacity, &generation(Some(2_500)), true);
        assert_eq!(normal.input_budget, 3_212);
        assert_eq!(deep.input_budget, 1_212);
        assert_eq!(
            ContextBudget::for_generation(&capacity, &generation(Some(9_999)), false).input_budget,
            0
        );
    }

    #[test]
    fn estimator_is_deterministic_and_context_error_is_preflight() {
        let estimator = ConservativeTokenEstimator;
        assert_eq!(
            estimator.estimate("λ fn long_identifier() {}"),
            estimator.estimate("λ fn long_identifier() {}")
        );
        let budget = ContextBudget {
            total: 1_000,
            output_reserve: 500,
            safety_margin: 384,
            input_budget: 116,
        };
        assert!(PromptPlan::check(&"x".repeat(1_000), budget, &estimator).is_err());
    }
}
