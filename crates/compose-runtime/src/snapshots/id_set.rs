//! A set of snapshot ids, as Compose's `SnapshotIdSet`: immutable and cheap to clone. Sorted
//! disjoint ranges instead of Compose's bit windows (D770): what a snapshot's invalid set holds is
//! a few runs of consecutive ids (`add_range`), and a walk through a state's records asks it only
//! when a record's id is above the snapshot's lowest invalid one.

use std::rc::Rc;

use super::SnapshotId;

/// Half-open ranges, sorted, never touching. `None` is empty, so the empty set needs no
/// allocation and can be a constant.
#[derive(Clone, Default)]
pub(crate) struct IdSet {
    ranges: Option<Rc<[(SnapshotId, SnapshotId)]>>,
}

impl IdSet {
    pub(crate) const EMPTY: IdSet = IdSet { ranges: None };

    fn of(ranges: Vec<(SnapshotId, SnapshotId)>) -> IdSet {
        if ranges.is_empty() { IdSet::EMPTY } else { IdSet { ranges: Some(Rc::from(ranges)) } }
    }

    fn ranges(&self) -> &[(SnapshotId, SnapshotId)] {
        self.ranges.as_deref().unwrap_or(&[])
    }

    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.ranges.is_none()
    }

    /// Whether `id` is in the set.
    #[inline]
    pub(crate) fn get(&self, id: SnapshotId) -> bool {
        let Some(ranges) = &self.ranges else { return false };
        let index = ranges.partition_point(|&(_, until)| until <= id);
        ranges.get(index).is_some_and(|&(from, _)| from <= id)
    }

    /// The set with `id`.
    pub(crate) fn set(&self, id: SnapshotId) -> IdSet {
        self.add_range(id, id.saturating_add(1))
    }

    /// The set without `id`.
    pub(crate) fn clear(&self, id: SnapshotId) -> IdSet {
        self.remove_range(id, id.saturating_add(1))
    }

    /// The set with every id from `from` until `until`.
    pub(crate) fn add_range(&self, from: SnapshotId, until: SnapshotId) -> IdSet {
        if from >= until {
            return self.clone();
        }
        let ranges = self.ranges();
        // What touches or overlaps the range: from the first ending at or after `from`, to the
        // last starting at or before `until`.
        let start = ranges.partition_point(|&(_, end)| end < from);
        let end = ranges.partition_point(|&(begin, _)| begin <= until);
        if start < end && ranges[start].0 <= from && until <= ranges[end - 1].1 && end - start == 1
        {
            return self.clone();
        }
        let mut merged = (from, until);
        if start < end {
            merged.0 = merged.0.min(ranges[start].0);
            merged.1 = merged.1.max(ranges[end - 1].1);
        }
        let mut result = Vec::with_capacity(ranges.len() + 1 - (end - start));
        result.extend_from_slice(&ranges[..start]);
        result.push(merged);
        result.extend_from_slice(&ranges[end..]);
        IdSet::of(result)
    }

    fn remove_range(&self, from: SnapshotId, until: SnapshotId) -> IdSet {
        let ranges = self.ranges();
        let start = ranges.partition_point(|&(_, end)| end <= from);
        let end = ranges.partition_point(|&(begin, _)| begin < until);
        if start >= end {
            return self.clone();
        }
        let mut result = Vec::with_capacity(ranges.len() + 1);
        result.extend_from_slice(&ranges[..start]);
        if ranges[start].0 < from {
            result.push((ranges[start].0, from));
        }
        if until < ranges[end - 1].1 {
            result.push((until, ranges[end - 1].1));
        }
        result.extend_from_slice(&ranges[end..]);
        IdSet::of(result)
    }

    /// The set with the ids of `other` too.
    pub(crate) fn or(&self, other: &IdSet) -> IdSet {
        if self.is_empty() {
            return other.clone();
        }
        other.ranges().iter().fold(self.clone(), |set, &(from, until)| set.add_range(from, until))
    }

    /// The set without the ids of `other`.
    pub(crate) fn and_not(&self, other: &IdSet) -> IdSet {
        other
            .ranges()
            .iter()
            .fold(self.clone(), |set, &(from, until)| set.remove_range(from, until))
    }

    /// The lowest id in the set, or `default`.
    pub(crate) fn lowest(&self, default: SnapshotId) -> SnapshotId {
        self.ranges().first().map_or(default, |&(from, _)| from)
    }

    /// The highest id in the set, or `default`.
    pub(crate) fn highest(&self, default: SnapshotId) -> SnapshotId {
        self.ranges().last().map_or(default, |&(_, until)| until - 1)
    }

    /// How many ids it holds.
    pub(crate) fn len(&self) -> u64 {
        self.ranges().iter().map(|&(from, until)| until - from).sum()
    }

    #[cfg(test)]
    fn iter(&self) -> impl Iterator<Item = SnapshotId> + '_ {
        self.ranges().iter().flat_map(|&(from, until)| from..until)
    }
}

impl std::fmt::Debug for IdSet {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_list().entries(self.ranges()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small deterministic generator, as the Compose tests' seeded `Random`.
    struct Random(u64);

    impl Random {
        fn next(&mut self, bound: u64) -> u64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 33) % bound
        }
    }

    fn set_up_to(times: u64, keep: impl Fn(u64) -> bool) -> IdSet {
        (0..=times).fold(IdSet::EMPTY, |set, id| if keep(id) { set.set(id) } else { set })
    }

    // Compose: SnapshotIdSetTests.emptySetShouldBeEmpty
    #[test]
    fn empty_set_should_be_empty() {
        assert!((0..1000).all(|id| !IdSet::EMPTY.get(id)));
    }

    // Compose: SnapshotIdSetTests.shouldBeAbleToSetItems
    #[test]
    fn should_be_able_to_set_items() {
        let set = set_up_to(10000, |_| true);
        assert!((0..10000).all(|id| set.get(id)));
    }

    // Compose: SnapshotIdSetTests.shouldBeAbleToSetOnlyEven, shouldBeAbleToSetOnlyOdds
    #[test]
    fn should_be_able_to_set_only_evens_or_odds() {
        for remainder in [0, 1] {
            let set = set_up_to(10000, |id| id % 2 == remainder);
            assert!((0..10000).all(|id| set.get(id) == (id % 2 == remainder)));
        }
    }

    // Compose: SnapshotIdSetTests.shouldBeAbleToClearEvens
    #[test]
    fn should_be_able_to_clear_evens() {
        let all = set_up_to(10000, |_| true);
        let set = (0..=10000).filter(|id| id % 2 == 0).fold(all, |set, id| set.clear(id));
        assert!((0..9999).all(|id| set.get(id) == (id % 2 == 1)));
    }

    // Compose: SnapshotIdSetTests.shouldBeAbleToCrawlSet
    #[test]
    fn should_be_able_to_crawl_set() {
        let set =
            (0..=10000u64).fold(IdSet::EMPTY, |set, id| set.clear(id.wrapping_sub(1)).set(id));
        assert!(set.get(10000));
        assert!((0..9999).all(|id| !set.get(id)));
    }

    // Compose: SnapshotIdSetTests.shouldBeAbleToCrawlAndClear
    #[test]
    fn should_be_able_to_crawl_and_clear() {
        let set = (0..=10000u64).fold(IdSet::EMPTY, |set, id| {
            let set =
                if id == 0 || (id - 1) % 33 != 0 { set.clear(id.wrapping_sub(1)) } else { set };
            set.set(id)
        });
        assert!(set.get(10000));
        assert!((0..9999).all(|id| set.get(id) == (id % 33 == 0)));
        let cleared = (0..10000).fold(set, |set, id| set.clear(id));
        assert!(cleared.get(10000));
        assert!((0..9999).all(|id| !cleared.get(id)));
    }

    // Compose: SnapshotIdSetTests.shouldBeAbleToInsertAndRemoveOutOfOptimalRange
    #[test]
    fn should_be_able_to_insert_and_remove_out_of_optimal_range() {
        let set = IdSet::EMPTY.set(1000).set(1);
        assert!(set.get(1000) && set.get(1));
        let set = set.set(10);
        assert!(set.get(10));
        let set = set.set(4);
        assert!(set.get(4));
        let set = set.clear(1);
        assert!(!set.get(1));
        let set = set.clear(4);
        assert!(!set.get(4));
        let set = set.clear(10);
        assert!(!set.get(1) && !set.get(4) && !set.get(10) && set.get(1000));
    }

    // Compose: SnapshotIdSetTests.shouldMatchBooleanArray
    #[test]
    fn should_match_boolean_array() {
        let mut random = Random(10);
        let mut booleans = [false; 1000];
        let mut set = IdSet::EMPTY;
        for _ in 0..=100 {
            let value = random.next(1000);
            booleans[value as usize] = true;
            set = set.set(value);
        }
        for _ in 0..=100 {
            let value = random.next(1000);
            booleans[value as usize] = false;
            set = set.clear(value);
        }
        assert!((0..1000).all(|id| set.get(id) == booleans[id as usize]));
    }

    // Compose: SnapshotIdSetTests.shouldBeAbleToAndNotBits, shouldBeAbleToAndNot
    #[test]
    fn should_be_able_to_and_not() {
        for size in [32, 64, 128, 512, 1024] {
            let mut random = Random(size);
            let mut booleans = [false; 1024];
            let (mut a, mut b) = (IdSet::EMPTY, IdSet::EMPTY);
            for id in 0..size {
                if random.next(1000) > 500 {
                    booleans[id as usize] = true;
                    a = a.set(id);
                }
            }
            for id in 0..size {
                if random.next(1000) > 500 {
                    booleans[id as usize] = false;
                    b = b.set(id);
                }
            }
            let set = a.and_not(&b);
            assert!((0..size).all(|id| set.get(id) == booleans[id as usize]));
        }
    }

    // Compose: SnapshotIdSetTests.shouldBeAbleToOr
    #[test]
    fn should_be_able_to_or() {
        for size in [32, 64, 128, 512, 1024] {
            let mut random = Random(size);
            let mut booleans = [false; 1024];
            let (mut a, mut b) = (IdSet::EMPTY, IdSet::EMPTY);
            for id in 0..size {
                if random.next(1000) > 500 {
                    booleans[id as usize] = true;
                    a = a.set(id);
                }
            }
            for id in 0..size {
                if random.next(1000) > 500 {
                    booleans[id as usize] = true;
                    b = b.set(id);
                }
            }
            let set = a.or(&b);
            assert!((0..size).all(|id| set.get(id) == booleans[id as usize]));
        }
    }

    // Compose: SnapshotIdSetTests.shouldBeAbleToIterate
    #[test]
    fn should_be_able_to_iterate() {
        for size in [64, 128, 512, 1024] {
            let mut random = Random(size);
            let mut values = Vec::new();
            let mut set = IdSet::EMPTY;
            for id in 0..size {
                if random.next(1000) > 500 {
                    values.push(id);
                    set = set.set(id);
                }
            }
            assert_eq!(set.iter().collect::<Vec<_>>(), values);
            assert_eq!(set.len(), values.len() as u64);
        }
    }

    // Compose: SnapshotIdSetTests.shouldReportTheCorrectLowest
    #[test]
    fn should_report_the_correct_lowest() {
        for number in 0..64 {
            assert_eq!(IdSet::EMPTY.set(number).lowest(u64::MAX), number);
        }
        assert_eq!(IdSet::EMPTY.lowest(7), 7);
    }

    // Compose: SnapshotIdSetTests.shouldOverflowGracefully
    #[test]
    fn should_overflow_gracefully() {
        let set = IdSet::EMPTY.set(0).set(i64::MAX as u64);
        assert!(set.get(0));
        assert!(set.get(i64::MAX as u64));
        assert!(!set.get(1));
    }

    // Compose: SnapshotIdSetTests.clearLastBitReturnsEmpty
    #[test]
    fn clear_last_id_gives_the_empty_set() {
        for id in [10, 70, 500, 191] {
            assert!(IdSet::EMPTY.set(id).clear(id).is_empty());
        }
        assert!(IdSet::EMPTY.set(500).set(1).clear(500).clear(1).is_empty());
        let a = IdSet::EMPTY.set(200).set(201);
        assert!(a.and_not(&a).is_empty());
        assert_eq!(a.and_not(&IdSet::EMPTY.set(202)).len(), 2);
        assert_eq!(a.or(&IdSet::EMPTY.set(200)).len(), 2);
        assert_eq!(IdSet::EMPTY.set(200).or(&a).len(), 2);
    }

    // Compose: SnapshotIdSetTests.andNotWithSharedBelowBoundClearsBelowBound
    #[test]
    fn and_not_with_a_shared_part_clears_it() {
        let base = IdSet::EMPTY.set(1).set(200);
        assert!(base.and_not(&base).is_empty());
        let a = base.set(201);
        let b = base.set(202);
        let difference = a.and_not(&b);
        assert!(!difference.get(1) && !difference.get(200));
        assert!(difference.get(201) && !difference.get(202));
        let only_in_b = base.clear(200).set(202);
        let difference = a.and_not(&only_in_b);
        assert!(!difference.get(1) && difference.get(200));
        assert!(difference.get(201) && !difference.get(202));
    }

    // Compose: SnapshotIdSetTests.shouldValidWhenSetIsLarge, with generated operations for its table
    #[test]
    fn should_stay_valid_when_the_set_is_large() {
        let mut random = Random(5);
        let mut model = std::collections::BTreeSet::new();
        let mut set = IdSet::EMPTY;
        for _ in 0..20000 {
            let id = random.next(4096);
            if random.next(2) == 0 {
                model.insert(id);
                set = set.set(id);
            } else {
                model.remove(&id);
                set = set.clear(id);
            }
        }
        assert!(set.iter().eq(model.iter().copied()));
        assert_eq!(set.len(), model.len() as u64);
        assert_eq!(set.lowest(u64::MAX), model.first().copied().unwrap_or(u64::MAX));
        assert_eq!(set.highest(0), model.last().copied().unwrap_or(0));
    }

    #[test]
    fn ranges_join_and_split() {
        let set = IdSet::EMPTY.add_range(5, 10).add_range(12, 15);
        assert_eq!(set.len(), 8);
        // Touching ranges join.
        let joined = set.add_range(10, 12);
        assert_eq!(joined.ranges(), &[(5, 15)]);
        // A contained range changes nothing; an empty one neither.
        assert_eq!(joined.add_range(6, 9).ranges(), &[(5, 15)]);
        assert_eq!(joined.add_range(9, 9).ranges(), &[(5, 15)]);
        // A range covering several takes them all.
        assert_eq!(set.add_range(1, 20).ranges(), &[(1, 20)]);
        // A removal in the middle splits.
        assert_eq!(joined.clear(9).ranges(), &[(5, 9), (10, 15)]);
        assert_eq!(joined.highest(0), 14);
        assert_eq!(IdSet::EMPTY.highest(3), 3);
        assert_eq!(set.and_not(&IdSet::EMPTY.add_range(7, 13)).ranges(), &[(5, 7), (13, 15)]);
    }
}
