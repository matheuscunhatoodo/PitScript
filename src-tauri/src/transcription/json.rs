use super::alignment::TimedText;
use serde::Deserialize;

#[derive(Deserialize)]
struct Output {
    transcription: Vec<Chunk>,
}
#[derive(Deserialize, Clone, Copy)]
struct Offsets {
    from: i64,
    to: i64,
}
#[derive(Deserialize)]
struct Token {
    id: i32,
    text: String,
    offsets: Option<Offsets>,
}
#[derive(Deserialize)]
struct Chunk {
    offsets: Offsets,
    text: String,
    #[serde(default)]
    tokens: Vec<Token>,
}

pub fn parse(text: &str) -> Result<Vec<TimedText>, String> {
    let output: Output = serde_json::from_str(text).map_err(|error| error.to_string())?;
    let mut result = Vec::new();
    for chunk in output.transcription {
        if chunk.text.trim().is_empty() {
            continue;
        }
        if chunk.offsets.from < 0 || chunk.offsets.to <= chunk.offsets.from {
            return Err("Invalid Whisper segment timestamps".into());
        }
        let tokens: Vec<_> = chunk
            .tokens
            .into_iter()
            .filter(|token| token.id >= 0 && token.id < 50257)
            .collect();
        let reconstructed = tokens
            .iter()
            .map(|token| token.text.as_str())
            .collect::<String>();
        if tokens.is_empty()
            || reconstructed.trim() != chunk.text.trim()
            || tokens.iter().any(|token| {
                token.offsets.is_none_or(|offset| {
                    offset.from < chunk.offsets.from
                        || offset.to < offset.from
                        || offset.from > chunk.offsets.to
                        || offset.to > chunk.offsets.to
                })
            })
        {
            result.push(TimedText {
                start_ms: chunk.offsets.from,
                end_ms: chunk.offsets.to,
                text: chunk.text,
                precise: false,
            });
            continue;
        }
        let mut words: Vec<TimedText> = Vec::new();
        for token in tokens {
            let offset = token.offsets.unwrap();
            let end = offset.to.max(offset.from + 1).min(chunk.offsets.to);
            if token.text.starts_with(char::is_whitespace) || words.is_empty() {
                words.push(TimedText {
                    start_ms: offset.from,
                    end_ms: end,
                    text: token.text,
                    precise: true,
                });
            } else if let Some(last) = words.last_mut() {
                last.text.push_str(&token.text);
                last.end_ms = last.end_ms.max(end);
            }
        }
        if words.iter().any(|word| word.end_ms <= word.start_ms) {
            result.push(TimedText {
                start_ms: chunk.offsets.from,
                end_ms: chunk.offsets.to,
                text: chunk.text,
                precise: false,
            });
        } else {
            result.extend(words);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_words_and_preserves_text_when_token_times_are_missing() {
        let json = r#"{"transcription":[{"offsets":{"from":0,"to":2000},"text":" Olá ação!","tokens":[{"id":50364,"text":"[_BEG_]"},{"id":1,"text":" Olá","offsets":{"from":10,"to":500}},{"id":2,"text":" a","offsets":{"from":500,"to":600}},{"id":3,"text":"ção","offsets":{"from":600,"to":900}},{"id":4,"text":"!","offsets":{"from":900,"to":1000}}]},{"offsets":{"from":3000,"to":4000},"text":" Outro texto","tokens":[{"id":1,"text":" Outro"}]}]}"#;
        let result = super::parse(json).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[1].text, " ação!");
        assert_eq!(result[1].start_ms, 500);
        assert_eq!(result[1].end_ms, 1000);
        assert!(result[0].precise);
        assert!(!result[2].precise);
        assert_eq!(result[2].text, " Outro texto");
        let boundary=super::parse(r#"{"transcription":[{"offsets":{"from":0,"to":1000},"text":" fala.","tokens":[{"id":1,"text":" fala","offsets":{"from":0,"to":1000}},{"id":13,"text":".","offsets":{"from":1000,"to":1000}}]}]}"#).unwrap();
        assert_eq!(boundary.len(), 1);
        assert!(boundary[0].precise);
        assert_eq!(boundary[0].text, " fala.");
        assert!(super::parse("{}").is_err());
        assert!(
            super::parse(r#"{"transcription":[{"offsets":{"from":2,"to":1},"text":"text"}]}"#)
                .is_err()
        );
    }
}
