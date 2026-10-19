//! Deterministic word rules. All vocabulary is supplied by the caller.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Vocabulary {
    backchannels: Vec<Vec<String>>,
    stop_phrases: Vec<Vec<String>>,
    spoken_words: Vec<String>,
}

impl Vocabulary {
    pub fn new<B, S, P>(backchannels: B, stop_phrases: S, spoken_words: P) -> Self
    where
        B: IntoIterator,
        B::Item: AsRef<str>,
        S: IntoIterator,
        S::Item: AsRef<str>,
        P: IntoIterator,
        P::Item: AsRef<str>,
    {
        Self {
            backchannels: backchannels
                .into_iter()
                .map(|value| normalize(value.as_ref()))
                .filter(|words| !words.is_empty())
                .collect(),
            stop_phrases: stop_phrases
                .into_iter()
                .map(|value| normalize(value.as_ref()))
                .filter(|words| !words.is_empty())
                .collect(),
            spoken_words: spoken_words
                .into_iter()
                .flat_map(|value| normalize(value.as_ref()))
                .collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WordDecision {
    Ignore,
    Backchannel,
    Echo,
    Stop,
    Barge,
}

pub fn normalize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .replace(['-', '’'], " ")
        .replace('\'', "")
        .split_whitespace()
        .map(|word| {
            word.trim_matches(|character: char| !character.is_alphanumeric())
                .to_string()
        })
        .filter(|word| !word.is_empty())
        .collect()
}

pub fn classify_barge(words: &str, vocabulary: &Vocabulary) -> WordDecision {
    let normalized = normalize(words);
    if normalized.is_empty() {
        return WordDecision::Ignore;
    }
    let is_backchannel = vocabulary
        .backchannels
        .iter()
        .any(|phrase| phrase == &normalized)
        || normalized.iter().all(|word| {
            vocabulary
                .backchannels
                .iter()
                .any(|phrase| phrase.len() == 1 && phrase[0].as_str() == word.as_str())
        });
    if vocabulary
        .stop_phrases
        .iter()
        .any(|phrase| normalized.starts_with(phrase))
    {
        return WordDecision::Stop;
    }
    if is_backchannel {
        return WordDecision::Backchannel;
    }
    let content_count = normalized
        .iter()
        .filter(|word| {
            !vocabulary
                .backchannels
                .iter()
                .any(|phrase| phrase.len() == 1 && phrase[0].as_str() == word.as_str())
        })
        .count();
    if normalized.len() >= 2 && !vocabulary.spoken_words.is_empty() {
        let inside = normalized
            .iter()
            .filter(|word| vocabulary.spoken_words.contains(word))
            .count();
        if inside as f32 / normalized.len() as f32 >= 0.6 {
            return WordDecision::Echo;
        }
    }
    if content_count >= 2 {
        WordDecision::Barge
    } else {
        WordDecision::Ignore
    }
}
