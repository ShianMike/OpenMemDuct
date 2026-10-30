use crate::models::MemorySnapshot;
use std::collections::VecDeque;

// Keep this retention contract in sync with src/lib/history.ts. No disk storage:
// the tray retains at most 48 hours of actual readings from this app session.
pub const HISTORY_DURATION_MS: u64 = 48 * 60 * 60_000;
pub const HISTORY_RECENT_DURATION_MS: u64 = 60 * 60_000;
const RECENT_BUCKET_MS: u64 = 1000;
const ARCHIVE_BUCKET_MS: u64 = 5 * 60_000;
const ARCHIVE_BUCKET_SAMPLES: usize = 11;
// Include the partial buckets at both age cutoffs: at most 9,816 snapshots.
const HISTORY_LIMIT: usize = (HISTORY_RECENT_DURATION_MS / RECENT_BUCKET_MS + 1) as usize
    + (((HISTORY_DURATION_MS - HISTORY_RECENT_DURATION_MS).div_ceil(ARCHIVE_BUCKET_MS) + 1)
        as usize)
        * ARCHIVE_BUCKET_SAMPLES;

pub fn push_history(history: &mut VecDeque<MemorySnapshot>, snapshot: MemorySnapshot) {
    if let Some(last) = history.back() {
        if snapshot.sequence <= last.sequence {
            return;
        }
        // A newer sequence remains current when the system clock moves backwards.
        if snapshot.timestamp_ms < last.timestamp_ms {
            history.clear();
        }
    }
    history.push_back(snapshot);
    compact(history);
}

fn archive_bucket(
    history: &VecDeque<MemorySnapshot>,
    first: usize,
    end: usize,
    retained: &mut Vec<usize>,
) {
    let mut minimum: [Option<(u64, usize)>; 3] = [None; 3];
    let mut maximum: [Option<(u64, usize)>; 3] = [None; 3];
    let mut missing = [None; 3];
    for (index, sample) in history.iter().enumerate().take(end).skip(first) {
        let values = sample.metrics.as_ref().map_or([None; 3], |metrics| {
            [
                metrics
                    .physical_total
                    .checked_sub(metrics.physical_available),
                metrics.pagefile_used,
                metrics.system_working_set,
            ]
        });
        for (series, value) in values.into_iter().enumerate() {
            if let Some(value) = value {
                if minimum[series].is_none_or(|(low, _)| value < low) {
                    minimum[series] = Some((value, index));
                }
                if maximum[series].is_none_or(|(high, _)| value > high) {
                    maximum[series] = Some((value, index));
                }
            } else if missing[series].is_none() {
                missing[series] = Some(index);
            }
        }
    }
    let mut indices = Vec::with_capacity(ARCHIVE_BUCKET_SAMPLES);
    indices.extend([first, end - 1]);
    indices.extend(minimum.into_iter().flatten().map(|(_, index)| index));
    indices.extend(maximum.into_iter().flatten().map(|(_, index)| index));
    indices.extend(missing.into_iter().flatten());
    indices.sort_unstable();
    indices.dedup();
    retained.extend(indices);
}

fn compact(history: &mut VecDeque<MemorySnapshot>) {
    let Some(latest) = history.back().map(|sample| sample.timestamp_ms) else {
        return;
    };
    let cutoff = latest.saturating_sub(HISTORY_DURATION_MS);
    let recent = latest.saturating_sub(HISTORY_RECENT_DURATION_MS);
    while history
        .front()
        .is_some_and(|sample| sample.timestamp_ms < cutoff)
    {
        history.pop_front();
    }
    let mut retained = Vec::with_capacity(history.len().min(HISTORY_LIMIT));
    let mut index = 0;
    while index < history.len() && history[index].timestamp_ms < recent {
        let bucket = history[index].timestamp_ms / ARCHIVE_BUCKET_MS;
        let mut end = index + 1;
        while end < history.len()
            && history[end].timestamp_ms < recent
            && history[end].timestamp_ms / ARCHIVE_BUCKET_MS == bucket
        {
            end += 1;
        }
        // First/last, min/max used bytes for all three series, and each series'
        // first unavailable reading. Keep real metrics, timestamps and order.
        archive_bucket(history, index, end, &mut retained);
        index = end;
    }
    if index == 0 {
        retained.push(0);
    }
    while index < history.len() {
        let bucket = history[index].timestamp_ms / RECENT_BUCKET_MS;
        let mut end = index + 1;
        while end < history.len() && history[end].timestamp_ms / RECENT_BUCKET_MS == bucket {
            end += 1;
        }
        if retained.last() != Some(&(end - 1)) {
            retained.push(end - 1);
        }
        index = end;
    }
    debug_assert!(retained.len() <= HISTORY_LIMIT);
    // Retain in place: no cloning of full snapshots or their error strings.
    let mut retained = retained.into_iter().peekable();
    let mut index = 0;
    history.retain(|_| {
        let keep = retained.peek() == Some(&index);
        if keep {
            retained.next();
        }
        index += 1;
        keep
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(sequence: u64, timestamp_ms: u64) -> MemorySnapshot {
        let mut snapshot: MemorySnapshot =
            serde_json::from_str(include_str!("../../tests/fixtures/snapshot.json")).unwrap();
        snapshot.sequence = sequence;
        snapshot.timestamp_ms = timestamp_ms;
        snapshot.elapsed_ms = timestamp_ms;
        snapshot
    }

    #[test]
    fn recent_history_keeps_first_last_and_bounds_subsecond_bursts() {
        let mut history = VecDeque::new();
        let first = sample(0, 0);
        push_history(&mut history, first.clone());
        for sequence in 1..10_000 {
            push_history(&mut history, sample(sequence, sequence));
        }
        assert_eq!(history.len(), 11);
        assert_eq!(history.front(), Some(&first));
        assert_eq!(history.back().unwrap().sequence, 9999);
        let previous = history.clone();
        push_history(&mut history, sample(9999, 10_000));
        push_history(&mut history, sample(9998, 1));
        assert_eq!(history, previous);
    }

    #[test]
    fn rollback_resets_history_and_sampling_resumes_with_monotonic_elapsed_time() {
        let mut history = VecDeque::from([sample(0, 60_000), sample(1, 61_000)]);
        let mut rollback = sample(2, 1000);
        rollback.elapsed_ms = 62_000;
        push_history(&mut history, rollback.clone());
        assert_eq!(history, VecDeque::from([rollback.clone()]));

        // Old sequences cannot restore future timestamps or trigger another reset.
        push_history(&mut history, sample(1, 63_000));
        push_history(&mut history, sample(2, 63_000));
        push_history(&mut history, sample(2, 0));
        assert_eq!(history, VecDeque::from([rollback.clone()]));

        for offset in 1..10_000 {
            let mut next = sample(rollback.sequence + offset, rollback.timestamp_ms + offset);
            next.elapsed_ms = rollback.elapsed_ms + offset;
            push_history(&mut history, next);
        }
        assert_eq!(history.len(), 11);
        assert!(history.len() <= HISTORY_LIMIT);
        assert_eq!(history.front(), Some(&rollback));
        assert_eq!(history.back().unwrap().sequence, 10_001);
        assert_eq!(history.back().unwrap().timestamp_ms, 10_999);
        assert!(history
            .iter()
            .zip(history.iter().skip(1))
            .all(|(previous, next)| previous.sequence < next.sequence
                && previous.timestamp_ms < next.timestamp_ms
                && previous.elapsed_ms < next.elapsed_ms));
    }

    #[test]
    fn compacts_two_days_without_discarding_the_oldest_or_recent_samples() {
        let fixture = sample(0, 0);
        let mut history: VecDeque<_> = (0..=HISTORY_DURATION_MS / 1000)
            .map(|sequence| MemorySnapshot {
                sequence,
                timestamp_ms: sequence * 1000,
                elapsed_ms: sequence * 1000,
                ..fixture.clone()
            })
            .collect();
        compact(&mut history);
        assert!(history.len() < 5000);
        assert!(history.len() <= HISTORY_LIMIT);
        assert_eq!(history.front().unwrap().timestamp_ms, 0);
        assert_eq!(history.back().unwrap().timestamp_ms, HISTORY_DURATION_MS);
        assert_eq!(
            history
                .iter()
                .filter(|sample| sample.timestamp_ms
                    >= HISTORY_DURATION_MS - HISTORY_RECENT_DURATION_MS)
                .count(),
            3601
        );
        let previous = history.clone();
        compact(&mut history);
        assert_eq!(history, previous);
    }

    #[test]
    fn preserves_independent_extrema_missing_counters_and_bucket_endpoints() {
        let mut history: VecDeque<_> = (0..12)
            .map(|sequence| {
                let mut snapshot = sample(sequence, sequence * 1000);
                let metrics = snapshot.metrics.as_mut().unwrap();
                metrics.physical_total = 1000;
                metrics.physical_available = match sequence {
                    1 => 100,
                    2 => 900,
                    _ => 500,
                };
                metrics.pagefile_total = Some(1000);
                metrics.pagefile_used = match sequence {
                    3 => Some(100),
                    4 => Some(900),
                    7 => None,
                    _ => Some(500),
                };
                if sequence == 7 {
                    metrics.pagefile_total = None;
                }
                metrics.system_working_set_peak = Some(1000);
                metrics.system_working_set = match sequence {
                    5 => Some(100),
                    6 => Some(900),
                    8 => None,
                    _ => Some(500),
                };
                if sequence == 8 {
                    metrics.system_working_set_peak = None;
                }
                if sequence == 9 {
                    snapshot.metrics = None;
                    snapshot.error = Some("Unavailable".into());
                }
                snapshot
            })
            .collect();
        let originals = history.clone();
        push_history(&mut history, sample(12, 2 * HISTORY_RECENT_DURATION_MS));
        assert_eq!(
            history
                .iter()
                .map(|sample| sample.sequence)
                .collect::<Vec<_>>(),
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 12]
        );
        for snapshot in history.iter().take(11) {
            assert_eq!(snapshot, &originals[snapshot.sequence as usize]);
        }
    }

    #[test]
    fn expires_only_samples_older_than_48_hours_and_retains_long_gaps() {
        let mut history = VecDeque::from([sample(0, 0), sample(1, 1000)]);
        push_history(&mut history, sample(2, HISTORY_DURATION_MS));
        assert_eq!(history.front().unwrap().timestamp_ms, 0);
        push_history(&mut history, sample(3, HISTORY_DURATION_MS + 1000));
        assert_eq!(history.front().unwrap().timestamp_ms, 1000);
        push_history(&mut history, sample(4, 3 * HISTORY_DURATION_MS));
        assert_eq!(history.len(), 1);
        assert_eq!(history.back().unwrap().sequence, 4);
    }

    #[test]
    fn incremental_compaction_matches_a_full_hour_moving_into_archive() {
        let fixture = sample(0, 0);
        let mut history: VecDeque<_> = (0..=3600)
            .map(|sequence| MemorySnapshot {
                sequence,
                timestamp_ms: sequence * 1000,
                elapsed_ms: sequence * 1000,
                ..fixture.clone()
            })
            .collect();
        for sequence in 3601..=3900 {
            push_history(&mut history, sample(sequence, sequence * 1000));
        }
        let mut bulk: VecDeque<_> = (0..=3900)
            .map(|sequence| sample(sequence, sequence * 1000))
            .collect();
        compact(&mut bulk);
        assert_eq!(history, bulk);
        assert_eq!(history.front().unwrap().sequence, 0);
        assert_eq!(history.back().unwrap().sequence, 3900);
    }
}
