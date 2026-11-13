//! The scripted model provider: what answers a prompt in M1.
//!
//! The product has no model to call yet, and it does not need one to be exercised end to end: a
//! provider that answers with a fixed sentence proves that the pipeline works, that the answer
//! is streamed in chunks and that the run completes. The chunks are the words of the answer, so
//! joining them gives the sentence back.

use boma_app::{AppError, ports::ModelProvider, ports::ProviderRequest, ports::ProviderTurn};

/// The answer the provider gives when it is built without one.
const DEFAULT_ANSWER: &str = "Boma Bot is ready to answer.";

/// A provider that answers with a scripted sentence, word by word.
#[derive(Debug, Clone)]
pub struct ScriptedProvider {
    answer: String,
}

impl ScriptedProvider {
    /// Builds a provider that answers `answer`.
    #[must_use]
    pub fn new(answer: impl Into<String>) -> Self {
        Self {
            answer: answer.into(),
        }
    }

    /// Builds a provider that answers the default sentence.
    #[must_use]
    pub fn with_default_answer() -> Self {
        Self::new(DEFAULT_ANSWER)
    }

    /// The sentence the provider answers.
    #[must_use]
    pub fn answer(&self) -> &str {
        &self.answer
    }
}

impl Default for ScriptedProvider {
    fn default() -> Self {
        Self::with_default_answer()
    }
}

#[async_trait::async_trait]
impl ModelProvider for ScriptedProvider {
    async fn complete(&self, _request: ProviderRequest) -> Result<ProviderTurn, AppError> {
        Ok(ProviderTurn::Chunks(chunks(&self.answer)))
    }
}

/// Splits an answer into the chunks the worker publishes one by one.
///
/// The chunks carry their trailing space, so joining them gives the answer back.
fn chunks(answer: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    for character in answer.chars() {
        current.push(character);
        if character == ' ' {
            parts.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::ScriptedProvider;

    #[test]
    fn req_020_the_scripted_answer_is_split_into_ordered_chunks() {
        let provider = ScriptedProvider::new("hello brave world");
        let chunks = super::chunks(provider.answer());
        assert_eq!(chunks, vec!["hello ", "brave ", "world"]);
        assert_eq!(chunks.concat(), "hello brave world");
    }
}
