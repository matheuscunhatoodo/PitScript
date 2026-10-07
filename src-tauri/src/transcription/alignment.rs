use crate::database::segments::NewSegment;

#[derive(Clone, Debug)]
pub struct TimedText {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub precise: bool,
}
#[derive(Clone, Debug)]
pub struct SpeakerTurn {
    pub start_ms: i64,
    pub end_ms: i64,
    pub speaker: i32,
    pub confidence: Option<f32>,
}

pub fn align(
    local: &[TimedText],
    remote: &[TimedText],
    turns: &[SpeakerTurn],
    local_offset: i64,
    remote_offset: i64,
) -> Vec<NewSegment> {
    let mut ordered: Vec<_> = turns.iter().collect();
    ordered.sort_by_key(|turn| (turn.start_ms, turn.speaker));
    let mut speakers = Vec::new();
    for turn in ordered {
        if !speakers.contains(&turn.speaker) {
            speakers.push(turn.speaker);
        }
    }
    let mut output = Vec::new();
    for (source, texts, offset) in [
        ("microphone", local, local_offset),
        ("system", remote, remote_offset),
    ] {
        for text in texts {
            if text.text.trim().is_empty() || text.start_ms < 0 || text.end_ms <= text.start_ms {
                continue;
            }
            let (label, confidence) = if source == "microphone" {
                ("Você".to_owned(), None)
            } else {
                let duration = text.end_ms - text.start_ms;
                let active: Vec<_> = turns
                    .iter()
                    .filter_map(|turn| {
                        let overlap =
                            text.end_ms.min(turn.end_ms) - text.start_ms.max(turn.start_ms);
                        (overlap > 0).then_some((turn, overlap))
                    })
                    .collect();
                let best = active.iter().max_by_key(|(_, overlap)| *overlap);
                match best {
                    Some((turn, overlap))
                        if text.precise
                            && *overlap as f64 / duration as f64 >= 0.6
                            && turn.confidence.is_some_and(|confidence| confidence >= 0.6)
                            && !active.iter().any(|(other, amount)| {
                                other.speaker != turn.speaker
                                    && *amount as f64 / duration as f64 >= 0.2
                            }) =>
                    {
                        let index = speakers
                            .iter()
                            .position(|speaker| *speaker == turn.speaker)
                            .unwrap();
                        (format!("Participante {}", index + 1), turn.confidence)
                    }
                    _ => ("Participantes".to_owned(), None),
                }
            };
            output.push(NewSegment {
                label,
                start_ms: text.start_ms.saturating_add(offset),
                end_ms: text.end_ms.saturating_add(offset),
                text: text.text.clone(),
                confidence,
                source: source.into(),
            });
        }
    }
    output.sort_by_key(|segment| segment.start_ms);
    let mut grouped: Vec<NewSegment> = Vec::new();
    for segment in output {
        if let Some(last) = grouped.last_mut() {
            if last.label == segment.label
                && last.source == segment.source
                && segment.start_ms <= last.end_ms + 700
                && segment.end_ms - last.start_ms <= 6000
            {
                last.end_ms = last.end_ms.max(segment.end_ms);
                if !segment.text.starts_with(char::is_whitespace) {
                    last.text.push(' ');
                }
                last.text.push_str(segment.text.trim_end());
                last.confidence = match (last.confidence, segment.confidence) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    _ => None,
                };
                continue;
            }
        }
        grouped.push(NewSegment {
            text: segment.text.trim().into(),
            ..segment
        });
    }
    grouped
}

#[cfg(test)]
mod tests {
    use super::*;
    fn text(start: i64, text: &str) -> TimedText {
        TimedText {
            start_ms: start,
            end_ms: start + 500,
            text: text.into(),
            precise: true,
        }
    }
    fn turn(start: i64, speaker: i32) -> SpeakerTurn {
        SpeakerTurn {
            start_ms: start,
            end_ms: start + 500,
            speaker,
            confidence: Some(0.9),
        }
    }
    #[test]
    fn detects_one_two_and_three_speakers_in_first_appearance_order() {
        for count in 1..=3 {
            let text: Vec<_> = (0..count).map(|i| text(i * 1000, " fala")).collect();
            let turns: Vec<_> = (0..count)
                .map(|i| turn(i * 1000, (10 - i) as i32))
                .collect();
            let result = align(&[], &text, &turns, 0, 0);
            assert_eq!(result.len(), count as usize);
            for (i, segment) in result.iter().enumerate() {
                assert_eq!(segment.label, format!("Participante {}", i + 1));
                assert_eq!(segment.text, "fala");
            }
        }
    }
    #[test]
    fn local_and_remote_alternate_with_shared_timeline_and_overlapping_local_voice() {
        let result = align(
            &[text(0, "Eu"), text(2000, "Eu de novo")],
            &[text(1000, "Remoto"), text(3000, "Mesmo remoto")],
            &[turn(1000, 8), turn(3000, 8)],
            200,
            0,
        );
        assert_eq!(
            result.iter().map(|s| s.label.as_str()).collect::<Vec<_>>(),
            vec!["Você", "Participante 1", "Você", "Participante 1"]
        );
        assert_eq!(result[0].start_ms, 200);
        let result = align(
            &[text(0, "local")],
            &[text(0, "remote")],
            &[turn(0, 1)],
            0,
            0,
        );
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].label, "Você");
        assert_eq!(result[1].label, "Participante 1");
    }
    #[test]
    fn overlap_low_confidence_missing_turn_and_imprecise_text_are_neutral() {
        let texts = vec![
            text(0, "overlap"),
            text(1000, "low"),
            text(2000, "missing"),
            TimedText {
                precise: false,
                ..text(3000, "coarse")
            },
        ];
        let turns = vec![
            turn(0, 1),
            SpeakerTurn {
                start_ms: 400,
                ..turn(0, 2)
            },
            SpeakerTurn {
                confidence: Some(0.1),
                ..turn(1000, 1)
            },
            turn(3000, 1),
        ];
        let result = align(&[], &texts, &turns, 0, 0);
        assert!(result.iter().all(|s| s.label == "Participantes"));
        assert!(result.iter().any(|s| s.text.contains("overlap")));
        assert!(result.iter().any(|s| s.text.contains("coarse")));
    }
}
