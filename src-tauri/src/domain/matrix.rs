use serde::{Deserialize, Serialize};

use super::comparison::ComparisonItem;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MatrixContestant {
    pub item: ComparisonItem,
    pub wins: usize,
    pub losses: usize,
    pub ties: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MatrixPair {
    pub left: usize,
    pub right: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MatrixSessionState {
    pub contestants: Vec<MatrixContestant>,
    pub pairs: Vec<MatrixPair>,
    pub current_index: usize,
    pub comparison_percent: u8,
}

impl MatrixSessionState {
    pub fn initialize(items: Vec<ComparisonItem>, comparison_percent: u8) -> Self {
        let contestants = items
            .into_iter()
            .map(|item| MatrixContestant {
                item,
                wins: 0,
                losses: 0,
                ties: 0,
            })
            .collect::<Vec<_>>();
        let mut pairs = sampled_pairs(contestants.len(), comparison_percent);
        shuffle(&mut pairs, fresh_seed());
        Self {
            contestants,
            pairs,
            current_index: 0,
            comparison_percent,
        }
    }

    pub fn completed(&self) -> bool {
        self.current_index >= self.pairs.len()
    }

    pub fn current_pair(&self) -> Option<&MatrixPair> {
        self.pairs.get(self.current_index)
    }
}

fn sampled_pairs(item_count: usize, comparison_percent: u8) -> Vec<MatrixPair> {
    if item_count < 2 {
        return Vec::new();
    }
    if comparison_percent == 100 {
        return (0..item_count)
            .flat_map(|left| ((left + 1)..item_count).map(move |right| MatrixPair { left, right }))
            .collect();
    }

    let opponent_count =
        (((item_count - 1) * comparison_percent as usize + 50) / 100).clamp(1, item_count - 1);
    let mut selected = std::collections::BTreeSet::new();
    let mut seed = fresh_seed();
    for item in 0..item_count {
        let mut opponents = (0..item_count)
            .filter(|candidate| *candidate != item)
            .collect::<Vec<_>>();
        shuffle(&mut opponents, seed);
        seed = next_random(seed);
        for opponent in opponents.into_iter().take(opponent_count) {
            selected.insert((item.min(opponent), item.max(opponent)));
        }
    }
    selected
        .into_iter()
        .map(|(left, right)| MatrixPair { left, right })
        .collect()
}

fn fresh_seed() -> u64 {
    let value = uuid::Uuid::new_v4().as_u128();
    (value as u64) ^ ((value >> 64) as u64)
}

fn next_random(mut value: u64) -> u64 {
    if value == 0 {
        value = 0x9e37_79b9_7f4a_7c15;
    }
    value ^= value << 13;
    value ^= value >> 7;
    value ^= value << 17;
    value
}

fn shuffle<T>(values: &mut [T], mut seed: u64) {
    for index in (1..values.len()).rev() {
        seed = next_random(seed);
        values.swap(index, (seed as usize) % (index + 1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_matrix_contains_every_unordered_pair_once() {
        let pairs = sampled_pairs(5, 100);
        assert_eq!(pairs.len(), 10);
        assert!(pairs.iter().all(|pair| pair.left < pair.right));
    }

    #[test]
    fn sampled_matrix_unions_each_objects_random_opponents() {
        let pairs = sampled_pairs(10, 50);
        assert!(pairs.len() > 10 * 9 / 4);
        assert!(pairs.len() <= 10 * 9 / 2);
    }
}
