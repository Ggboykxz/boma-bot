//! Picking the model a run executes on (REQ-015).
//!
//! The resolution order is the spec's: the bot's own pinned choice, then the space, then the
//! person, then the deployment default. A bot pinned to a model the deployment cannot reach
//! fails visibly with the model named — it never executes "something else" silently (REQ-015).

use serde::{Deserialize, Serialize};

use crate::error::DomainError;

/// The model a run will execute on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelChoice {
    /// The provider the model lives on: `anthropic`, `openai`, … as the deployment names it.
    pub provider: String,
    /// The model identifier.
    pub model: String,
    /// Whether the choice was pinned: a pinned model never falls back.
    pub pinned: bool,
}

/// The models this deployment can reach.
///
/// `default` names the fallback every run executes on when nothing else applies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeploymentModels {
    /// The catalogue keys the deployment implements, e.g. `anthropic/claude-sonnet`.
    pub reachable: Vec<String>,
    /// The fallback model.
    pub default: String,
}

/// Whether a catalogue key is reachable on this deployment.
fn reachable(models: &DeploymentModels, key: &str) -> bool {
    models.reachable.iter().any(|candidate| candidate == key)
}

/// The models, in preference order, the run may pick from: pinned bot, space, person, deployment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelPreferences {
    /// The bot's own choice, pinned when it is not allowed to fall back.
    pub bot: Option<(String, bool)>,
    /// The space's default chat model.
    pub space: Option<String>,
    /// The person's default chat model.
    pub person: Option<String>,
}

/// Picks the model a run executes on (REQ-015).
///
/// A pinned choice the deployment cannot reach is a visible failure: the run never executes
/// another model silently. An unpinned choice the deployment cannot reach falls back, and the
/// fallback is part of the audit trail.
///
/// # Errors
///
/// Returns [`DomainError::PinnedModelUnreachable`] when a pinned model is not reachable on this
/// deployment.
pub fn resolve(
    preferences: &ModelPreferences,
    models: &DeploymentModels,
) -> Result<ModelChoice, DomainError> {
    if let Some((key, true)) = &preferences.bot {
        if reachable(models, key) {
            return Ok(split(key, true));
        }
        return Err(DomainError::PinnedModelUnreachable(key.clone()));
    }
    for candidate in [
        preferences.bot.as_ref().map(|(key, _)| key.clone()),
        preferences.space.clone(),
        preferences.person.clone(),
        Some(models.default.clone()),
    ]
    .into_iter()
    .flatten()
    {
        if reachable(models, &candidate) {
            let pinned = matches!(&preferences.bot, Some((_, true)));
            return Ok(split(&candidate, pinned));
        }
    }
    // The deployment default is reachable by construction; the loop only fails if a deployment
    // declares a default it cannot reach, which is a refusal, not a fallback.
    Err(DomainError::PinnedModelUnreachable(models.default.clone()))
}

/// Splits `provider/model` into its two halves.
fn split(key: &str, pinned: bool) -> ModelChoice {
    match key.split_once('/') {
        Some((provider, model)) => ModelChoice {
            provider: provider.to_owned(),
            model: model.to_owned(),
            pinned,
        },
        None => ModelChoice {
            provider: "custom".to_owned(),
            model: key.to_owned(),
            pinned,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{DeploymentModels, ModelPreferences, resolve};
    use crate::error::DomainError;

    fn deployment() -> DeploymentModels {
        DeploymentModels {
            reachable: vec![
                "openai/gpt-5".to_owned(),
                "anthropic/claude-sonnet".to_owned(),
            ],
            default: "anthropic/claude-sonnet".to_owned(),
        }
    }

    #[test]
    fn req_015_falls_back_to_the_deployment_default_when_nothing_is_pinned() {
        let choice = resolve(&ModelPreferences::default(), &deployment())
            .expect("the deployment default is always reachable");
        assert_eq!(choice.provider, "anthropic");
        assert_eq!(choice.model, "claude-sonnet");
        assert!(!choice.pinned);
    }

    #[test]
    fn req_015_a_pinned_model_the_deployment_cannot_reach_is_a_visible_failure() {
        let error = resolve(
            &ModelPreferences {
                bot: Some(("openai/gpt-6".to_owned(), true)),
                space: None,
                person: None,
            },
            &deployment(),
        )
        .expect_err("the pinned model is unreachable");
        assert!(matches!(error, DomainError::PinnedModelUnreachable(_)));
    }

    #[test]
    fn req_015_the_bot_wins_over_the_deployment_when_reachable_and_unpinned() {
        let choice = resolve(
            &ModelPreferences {
                bot: Some(("openai/gpt-5".to_owned(), false)),
                space: None,
                person: None,
            },
            &deployment(),
        )
        .expect("the bot choice is reachable");
        assert_eq!(choice.provider, "openai");
        assert_eq!(choice.model, "gpt-5");
    }

    #[test]
    fn req_015_the_space_wins_over_the_deployment_when_the_bot_has_no_opinion() {
        let choice = resolve(
            &ModelPreferences {
                bot: None,
                space: Some("openai/gpt-5".to_owned()),
                person: None,
            },
            &deployment(),
        )
        .expect("the space choice is reachable");
        assert_eq!(choice.provider, "openai");
        assert_eq!(choice.model, "gpt-5");
    }

    #[test]
    fn req_015_an_unpinned_unreachable_choice_falls_back_instead() {
        let choice = resolve(
            &ModelPreferences {
                bot: Some(("openai/gpt-6".to_owned(), false)),
                space: None,
                person: None,
            },
            &deployment(),
        )
        .expect("an unpinned unreachable choice falls back");
        assert_eq!(choice.provider, "anthropic");
        assert_eq!(choice.model, "claude-sonnet");
    }
}
