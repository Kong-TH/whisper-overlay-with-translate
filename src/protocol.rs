use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Word {
    #[allow(unused)]
    pub begin: Option<f32>,
    #[allow(unused)]
    pub end: Option<f32>,
    #[serde(default)]
    pub word: String,
    #[serde(default)]
    pub probability: Option<f32>,
}

#[derive(Debug, Deserialize)]
pub struct Segment {
    #[allow(unused)]
    pub begin: Option<f32>,
    #[allow(unused)]
    pub end: Option<f32>,
    #[allow(unused)]
    pub text: Option<String>,
    #[serde(default)]
    pub words: Vec<Word>,
}

#[derive(Debug, Deserialize)]
pub struct ModelResult {
    pub kind: String,
    #[serde(default)]
    pub text: String,
    #[allow(unused)]
    pub translated_text: Option<String>,
    #[allow(unused)]
    pub source_text: Option<String>,
    #[serde(default)]
    pub segments: Vec<Segment>,
}

impl ModelResult {
    pub fn fallback_text(&self) -> &str {
        if self.text.is_empty() {
            self.translated_text
                .as_deref()
                .or(self.source_text.as_deref())
                .unwrap_or("")
        } else {
            &self.text
        }
    }

    pub fn has_word_segments(&self) -> bool {
        self.segments
            .iter()
            .any(|segment| !segment.words.is_empty())
    }
}
