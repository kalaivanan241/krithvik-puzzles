//! ElevenLabs text-to-speech for catalog voice clips.
//!
//! Phrase templates here must match the visual prompts in `src/game.ts`.

use axum::body::Bytes;
use std::{env, sync::Arc, time::Duration};
use tokio::sync::Semaphore;

const DEFAULT_BASE_URL: &str = "https://api.elevenlabs.io";
/// "Matilda", a warm premade voice.
const DEFAULT_VOICE_ID: &str = "XrExE9yKIg1WjnnlVkGX";
const DEFAULT_MODEL_ID: &str = "eleven_multilingual_v2";
/// Stay within the concurrent request limit of small ElevenLabs plans.
const MAX_CONCURRENT_REQUESTS: usize = 2;

#[derive(Clone)]
pub struct Speech {
    client: reqwest::Client,
    key: Arc<str>,
    endpoint: Arc<str>,
    model: Arc<str>,
    permits: Arc<Semaphore>,
}

impl Speech {
    /// Enabled only when `ELEVEN_LABS_API_KEY` is set and non-empty.
    pub fn from_env() -> Option<Self> {
        let key = env::var("ELEVEN_LABS_API_KEY").ok().filter(|key| !key.trim().is_empty())?;
        let setting = |name: &str, default: &str| env::var(name).ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| default.into());
        Self::new(
            key.trim(),
            &setting("ELEVEN_LABS_BASE_URL", DEFAULT_BASE_URL),
            &setting("ELEVEN_LABS_VOICE_ID", DEFAULT_VOICE_ID),
            &setting("ELEVEN_LABS_MODEL_ID", DEFAULT_MODEL_ID),
        )
    }

    pub fn new(key: &str, base_url: &str, voice_id: &str, model_id: &str) -> Option<Self> {
        let client = reqwest::Client::builder().timeout(Duration::from_secs(30)).build()
            .map_err(|error| tracing::error!(%error, "Could not create the ElevenLabs client")).ok()?;
        let voice: String = voice_id.chars().filter(char::is_ascii_alphanumeric).collect();
        Some(Self {
            client,
            key: key.into(),
            endpoint: format!("{}/v1/text-to-speech/{voice}?output_format=mp3_44100_128", base_url.trim_end_matches('/')).into(),
            model: model_id.into(),
            permits: Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS)),
        })
    }

    /// Returns MP3 audio for `text`. Errors never include the API key.
    pub async fn synthesize(&self, text: &str) -> Result<Bytes, String> {
        let _permit = self.permits.acquire().await.map_err(|e| e.to_string())?;
        let body = serde_json::json!({
            "text": text,
            "model_id": &*self.model,
            "voice_settings": {"stability": 0.5, "similarity_boost": 0.75, "speed": 0.9},
        });
        let response = self.client.post(&*self.endpoint)
            .header("xi-api-key", &*self.key)
            .header("accept", "audio/mpeg")
            .header("content-type", "application/json")
            .body(body.to_string())
            .send().await
            .map_err(|error| format!("ElevenLabs request failed: {}", error.without_url()))?;
        let status = response.status();
        if !status.is_success() { return Err(format!("ElevenLabs returned {status}")); }
        response.bytes().await.map_err(|error| format!("ElevenLabs response failed: {}", error.without_url()))
    }
}

/// Spoken phrases for an item, keyed by the slot the frontend plays them in.
pub fn item_phrases(name: &str, sound: &str) -> Vec<(&'static str, String)> {
    let name = name.trim().to_lowercase();
    let mut phrases = vec![
        ("prompt", format!("Where is the {name}?")),
        ("retry", format!("Try again! Where is the {name}?")),
        ("reveal", format!("That is the {name}.")),
    ];
    if !sound.trim().is_empty() { phrases.push(("sound", sound.trim().to_string())); }
    phrases
}

pub fn category_phrase(name: &str) -> String { name.trim().to_string() }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phrases_match_frontend_prompts() {
        let phrases = item_phrases(" Fire Engine ", "");
        assert_eq!(phrases, vec![
            ("prompt", "Where is the fire engine?".to_string()),
            ("retry", "Try again! Where is the fire engine?".to_string()),
            ("reveal", "That is the fire engine.".to_string()),
        ]);
        assert_eq!(item_phrases("Cat", "Meow!").last().unwrap(), &("sound", "Meow!".to_string()));
        assert_eq!(category_phrase(" Vehicles "), "Vehicles");
    }

    #[test]
    fn voice_id_cannot_alter_the_request_path() {
        let speech = Speech::new("key", "http://127.0.0.1:1/", "../x?y=1", "model").unwrap();
        assert_eq!(&*speech.endpoint, "http://127.0.0.1:1/v1/text-to-speech/xy1?output_format=mp3_44100_128");
    }
}
