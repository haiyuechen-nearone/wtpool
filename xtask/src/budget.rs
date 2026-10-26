//! How the latency budgets of verification.md, "Budgets", count their samples: the nearest rank
//! p95, and rounds that stop once their p95 is over the budget.

use std::time::Duration;

/// The 95th percentile of `samples`, nearest rank.
pub fn p95(samples: &[Duration]) -> Duration {
    let mut sorted = samples.to_vec();
    sorted.sort();
    let rank = p95_rank(sorted.len());
    sorted
        .get(rank.saturating_sub(1))
        .copied()
        .unwrap_or_default()
}

/// The nearest rank of the p95 among `samples` samples, at least 1.
fn p95_rank(samples: usize) -> usize {
    samples.saturating_mul(95).div_ceil(100).max(1)
}

/// The delay of each of `rounds` rounds of `round`, `None` for a change that never showed.
/// Stops once the p95 of the rounds can no longer be within `budget`, so a blind or slow watcher
/// fails with what it measured, well before the bench profile's 120 s limit ends the test.
pub fn delays_within(
    rounds: usize,
    budget: Duration,
    mut round: impl FnMut(usize) -> Option<Duration>,
) -> Vec<Option<Duration>> {
    // Rounds above the p95; one more decides it.
    let spare = rounds.saturating_sub(p95_rank(rounds));
    let mut delays = Vec::new();
    for index in 0..rounds {
        delays.push(round(index));
        let over = delays
            .iter()
            .filter(|delay| delay.is_none_or(|delay| delay > budget))
            .count();
        if over > spare {
            break;
        }
    }
    delays
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    #[rstest]
    #[case::none(&[], 0)]
    #[case::one(&[7], 7)]
    #[case::twenty_skip_the_slowest(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 900], 19)]
    #[case::nineteen_take_the_slowest(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 900], 900)]
    fn p95__should_take_the_nearest_rank(#[case] samples: &[u64], #[case] expected: u64) {
        // Given
        let samples: Vec<Duration> = samples.iter().copied().map(ms).collect();

        // When
        let found = p95(&samples);

        // Then
        assert_eq!(found, ms(expected));
    }

    #[rstest]
    #[case::all_within(&[Some(100); 20], 20)]
    #[case::one_slow(&[Some(5000), Some(100), Some(100)], 20)]
    #[case::two_slow(&[Some(100), Some(5000), Some(100), Some(5000)], 4)]
    #[case::two_unseen(&[None, None], 2)]
    #[case::unseen_and_slow(&[None, Some(100), Some(1001)], 3)]
    fn delays_within__should_stop_once_the_p95_is_over_budget(
        #[case] delays: &[Option<u64>],
        #[case] expected_rounds: usize,
    ) {
        // Given
        let budget = ms(1000);
        let at = |index: usize| delays.get(index).copied().unwrap_or(Some(100)).map(ms);

        // When
        let measured = delays_within(20, budget, at);

        // Then
        assert_eq!(measured.len(), expected_rounds, "{measured:?}");
        let flat: Vec<Duration> = measured.iter().copied().flatten().collect();
        let failed = measured.iter().any(Option::is_none) || p95(&flat) > budget;
        assert_eq!(failed, expected_rounds < 20, "{measured:?}");
    }
}
