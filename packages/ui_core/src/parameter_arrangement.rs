//! Reordering of workflow parameter lists. A *group* is a header item plus the items after it up to
//! the next separator; items before the first separator form a leading group without header.
//! The functions are generic over the item type so they work on plain values in tests and on
//! `Signal<Parameter>` in the editor.

use std::ops::Range;

/// Direction of a one step move
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Towards the start of the list
    Up,
    /// Towards the end of the list
    Down,
}

/// Range an item moves with: the whole group for a separator, else just the item.
///
/// # Arguments
/// * `items` - The list
/// * `index` - Index of the item being moved
/// * `is_separator` - Tells whether an item starts a group
pub fn moving_range<T>(
    items: &[T],
    index: usize,
    is_separator: impl Fn(&T) -> bool,
) -> Range<usize> {
    if !is_separator(&items[index]) {
        return index..index + 1;
    }
    let end = items[index + 1..]
        .iter()
        .position(&is_separator)
        .map_or(items.len(), |offset| index + 1 + offset);
    index..end
}

/// Splits the list into sections for display: the leading items without a group (if any), then
/// one range per group (header included).
///
/// # Arguments
/// * `items` - The list
/// * `is_separator` - Tells whether an item is a group header
pub fn sections<T>(items: &[T], is_separator: impl Fn(&T) -> bool) -> Vec<Range<usize>> {
    let mut sections = Vec::new();
    let mut start = 0;
    for (index, item) in items.iter().enumerate() {
        if is_separator(item) && index > start {
            sections.push(start..index);
            start = index;
        }
    }
    if start < items.len() {
        sections.push(start..items.len());
    }
    sections
}

/// Moves `range` so it ends up in front of the item that is at `destination` in the *original*
/// list (`items.len()` = at the end). A destination inside or directly around the range changes
/// nothing. Returns the new start index of the range.
///
/// # Arguments
/// * `items` - The list
/// * `range` - Items to move, must be within the list
/// * `destination` - Insert position in terms of the list before the move
pub fn move_range<T>(items: &mut Vec<T>, range: Range<usize>, destination: usize) -> usize {
    if destination >= range.start && destination <= range.end {
        return range.start;
    }
    let length = range.len();
    let moved: Vec<T> = items.drain(range.clone()).collect();
    let insert_at = if destination > range.end {
        destination - length
    } else {
        destination
    };
    for (offset, item) in moved.into_iter().enumerate() {
        items.insert(insert_at + offset, item);
    }
    insert_at
}

/// Moves the item at `index` one step (a separator moves its whole group one group). Returns the
/// new index of the moved item, unchanged if it is already at the border.
///
/// # Arguments
/// * `items` - The list
/// * `index` - Item to move
/// * `direction` - Up or down
/// * `is_separator` - Tells whether an item starts a group
pub fn step<T>(
    items: &mut Vec<T>,
    index: usize,
    direction: Direction,
    is_separator: impl Fn(&T) -> bool,
) -> usize {
    let range = moving_range(items, index, &is_separator);
    let is_group = is_separator(&items[index]);
    match direction {
        Direction::Up => {
            if range.start == 0 {
                return index;
            }
            let destination = if is_group {
                // Start of the previous group (the last separator before, or the list start)
                items[..range.start]
                    .iter()
                    .rposition(&is_separator)
                    .unwrap_or(0)
            } else {
                range.start - 1
            };
            move_range(items, range, destination)
        }
        Direction::Down => {
            if range.end >= items.len() {
                return index;
            }
            let destination = if is_group {
                // End of the next group
                moving_range(items, range.end, &is_separator).end
            } else {
                range.end + 1
            };
            move_range(items, range, destination)
        }
    }
}

/// Where a dragged range is inserted when dropped on the item at `target`: before it when dropping
/// upwards, after it when dropping downwards (after the whole group if a group is dropped on
/// another group's separator). Returns an insert position as taken by [`move_range`].
///
/// # Arguments
/// * `items` - The list
/// * `dragged` - Range being dragged
/// * `target` - Index of the item it is dropped on
/// * `is_separator` - Tells whether an item starts a group
pub fn drop_destination<T>(
    items: &[T],
    dragged: &Range<usize>,
    target: usize,
    is_separator: impl Fn(&T) -> bool,
) -> usize {
    if target < dragged.start {
        target
    } else if dragged.len() > 1 || is_separator(&items[dragged.start]) {
        if is_separator(&items[target]) {
            moving_range(items, target, &is_separator).end
        } else {
            target + 1
        }
    } else {
        target + 1
    }
}

/// Edit of a parameter list requested by the user in the editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParameterMove {
    /// Move the item (a separator with its group) one step
    Step(usize, Direction),
    /// Item (or group) `from` was dropped on the item `target` of the same list
    Drop {
        /// Index of the dragged item
        from: usize,
        /// Index of the item it was dropped on
        target: usize,
    },
    /// Move the item (a separator with its group) to the end of the other list
    ToOtherList(usize),
}

/// Names from `moving` that are non-empty and also in `existing`.
///
/// # Arguments
/// * `moving` - Names of the parameters about to be moved
/// * `existing` - Names already in the target list
pub fn name_clashes<'a>(
    moving: impl IntoIterator<Item = &'a str>,
    existing: &[&str],
) -> Vec<String> {
    moving
        .into_iter()
        .filter(|name| !name.is_empty() && existing.contains(name))
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Items starting with `#` are separators.
    fn sep(item: &&str) -> bool {
        item.starts_with('#')
    }

    /// A list with a leading item and two groups.
    fn list() -> Vec<&'static str> {
        vec!["a", "#G1", "b", "c", "#G2", "d"]
    }

    /// A separator's range covers its group, a plain item only itself.
    #[test]
    fn ranges() {
        let items = list();
        assert_eq!(moving_range(&items, 1, sep), 1..4);
        assert_eq!(moving_range(&items, 4, sep), 4..6);
        assert_eq!(moving_range(&items, 2, sep), 2..3);
    }

    /// A plain item swaps with its neighbour, also across a group border.
    #[test]
    fn step_item() {
        let mut items = list();
        assert_eq!(step(&mut items, 3, Direction::Up, sep), 2);
        assert_eq!(items, ["a", "#G1", "c", "b", "#G2", "d"]);
        assert_eq!(step(&mut items, 3, Direction::Down, sep), 4);
        assert_eq!(items, ["a", "#G1", "c", "#G2", "b", "d"]);
    }

    /// A separator moves its whole group past the neighbouring group.
    #[test]
    fn step_group() {
        let mut items = list();
        assert_eq!(step(&mut items, 4, Direction::Up, sep), 1);
        assert_eq!(items, ["a", "#G2", "d", "#G1", "b", "c"]);
        assert_eq!(step(&mut items, 1, Direction::Down, sep), 4);
        assert_eq!(items, ["a", "#G1", "b", "c", "#G2", "d"]);
    }

    /// The first group moves above the leading items that have no header.
    #[test]
    fn first_group_moves_above_leading_items() {
        let mut items = list();
        assert_eq!(step(&mut items, 1, Direction::Up, sep), 0);
        assert_eq!(items, ["#G1", "b", "c", "a", "#G2", "d"]);
    }

    /// Borders do nothing.
    #[test]
    fn borders() {
        let mut items = list();
        assert_eq!(step(&mut items, 0, Direction::Up, sep), 0);
        assert_eq!(step(&mut items, 4, Direction::Down, sep), 4);
        assert_eq!(items, list());
    }

    /// Dropping in front of an item, downwards and upwards, and onto itself.
    #[test]
    fn move_to_destination() {
        let mut items = list();
        assert_eq!(move_range(&mut items, 0..1, 4), 3);
        assert_eq!(items, ["#G1", "b", "c", "a", "#G2", "d"]);
        let mut items = list();
        assert_eq!(move_range(&mut items, 5..6, 2), 2);
        assert_eq!(items, ["a", "#G1", "d", "b", "c", "#G2"]);
        let mut items = list();
        assert_eq!(move_range(&mut items, 2..3, 3), 2);
        assert_eq!(items, list());
        assert_eq!(move_range(&mut items, 0..1, 6), 5);
        assert_eq!(items, ["#G1", "b", "c", "#G2", "d", "a"]);
    }

    /// Drop destinations for items and groups, up and down.
    #[test]
    fn drops() {
        let items = list();
        assert_eq!(drop_destination(&items, &(0..1), 2, sep), 3);
        assert_eq!(drop_destination(&items, &(5..6), 2, sep), 2);
        // group on another group's separator goes after that group
        assert_eq!(drop_destination(&items, &(1..4), 4, sep), 6);
        assert_eq!(drop_destination(&items, &(4..6), 1, sep), 1);
    }

    /// Sections: leading items, then each group with its header.
    #[test]
    fn section_split() {
        assert_eq!(sections(&list(), sep), [0..1, 1..4, 4..6]);
        assert_eq!(sections(&["#A", "#B"], sep), [0..1, 1..2]);
        assert!(sections::<&str>(&[], sep).is_empty());
    }

    /// Empty names never clash.
    #[test]
    fn clashes() {
        assert_eq!(name_clashes(["x", "", "y"], &["y", ""]), ["y"]);
    }
}
