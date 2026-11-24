use super::*;
use fsrs::{FSRS, MemoryState};

fn next(row: &CardRow, now: DateTime<Utc>) -> anyhow::Result<fsrs::NextStates> {
    let memory = row
        .stability
        .zip(row.difficulty)
        .map(|(stability, difficulty)| MemoryState {
            stability: stability as f32,
            difficulty: difficulty as f32,
        });
    let elapsed = row
        .last_review
        .map(|at| (now - at).num_days().max(0) as u32)
        .unwrap_or(0);
    Ok(FSRS::default().next_states(memory, 0.9, elapsed)?)
}
fn seconds(days: f32) -> i64 {
    (days.round().max(1.0) as i64).saturating_mul(86400)
}
fn intervals(row: &CardRow, states: &fsrs::NextStates) -> RatingIntervals {
    let good = seconds(states.good.interval);
    RatingIntervals {
        again: 60,
        hard: if row.review_count == 0 {
            600
        } else {
            seconds(states.hard.interval).min(good)
        },
        good,
        easy: seconds(states.easy.interval).max(good + 86400),
    }
}
pub(super) fn previews(row: &CardRow, now: DateTime<Utc>) -> anyhow::Result<RatingIntervals> {
    Ok(intervals(row, &next(row, now)?))
}
pub(super) fn rate(
    row: &CardRow,
    rating: Rating,
    now: DateTime<Utc>,
) -> anyhow::Result<ReviewResult> {
    let states = next(row, now)?;
    let intervals = intervals(row, &states);
    let (state, interval) = match rating {
        Rating::Again => (states.again, intervals.again),
        Rating::Hard => (states.hard, intervals.hard),
        Rating::Good => (states.good, intervals.good),
        Rating::Easy => (states.easy, intervals.easy),
    };
    Ok(ReviewResult {
        card_id: row.id,
        version: row.version + 1,
        due_at: now + chrono::Duration::seconds(interval),
        interval_seconds: interval,
        stability: state.memory.stability as f64,
        difficulty: state.memory.difficulty as f64,
        review_count: row.review_count + 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ratings_follow_fsrs_with_learning_steps() {
        let now = Utc::now();
        let mut row = CardRow {
            id: Uuid::new_v4(),
            version: 0,
            payload: CardPayload::Anki(anki_flashcards::AnkiCard {
                kind: anki_flashcards::KIND.into(),
                prompt: String::new(),
            }),
            due_at: now,
            review_count: 0,
            anki_review_count: 0,
            stability: None,
            difficulty: None,
            last_review: None,
            tags: vec![],
            tags_version: 0,
        };
        let p = previews(&row, now).unwrap();
        assert_eq!(p.again, 60);
        assert_eq!(p.hard, 600);
        assert!(p.good >= 86400);
        assert!(p.easy > p.good);
        let expected = FSRS::default().next_states(None, 0.9, 0).unwrap();
        let result = rate(&row, Rating::Good, now).unwrap();
        assert_eq!(result.stability, expected.good.memory.stability as f64);
        row.review_count = result.review_count;
        row.version = result.version;
        row.stability = Some(result.stability);
        row.difficulty = Some(result.difficulty);
        row.last_review = Some(now);
        let later = now + chrono::Duration::days(10);
        let p = previews(&row, later).unwrap();
        assert!(p.again < p.hard && p.hard <= p.good && p.good < p.easy);
        let result = rate(&row, Rating::Easy, later).unwrap();
        assert_eq!(result.due_at, later + chrono::Duration::seconds(p.easy));
        assert_eq!(result.version, 2);
    }
}
