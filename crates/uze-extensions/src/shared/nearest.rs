//! Which of the things on a board lies in a direction from a point — what
//! an arrow key means on a surface that is a picture rather than a list.

use crate::view::PanDirection;

/// The candidate nearest `from` in `direction`, each standing at the cell
/// it is paired with.
///
/// A row of cells is twice as tall as a column is wide, so a step down
/// counts double: "nearest" has to mean what it looks like. Straying
/// sideways costs twice what going on does, so the arrow follows the line
/// it points along before it follows the closest thing. With `ahead_only`
/// off — nothing picked yet, so `from` is merely the middle of the screen
/// — a candidate on any side will do. Ties go to the first candidate.
pub fn toward<T>(
    from: (i32, i32),
    direction: PanDirection,
    ahead_only: bool,
    candidates: impl IntoIterator<Item = (T, (i32, i32))>,
) -> Option<T> {
    candidates
        .into_iter()
        .filter_map(|(candidate, at)| {
            let (dx, dy) = (at.0 - from.0, (at.1 - from.1) * 2);
            let (along, across) = match direction {
                PanDirection::Left => (-dx, dy),
                PanDirection::Right => (dx, dy),
                PanDirection::Up => (-dy, dx),
                PanDirection::Down => (dy, dx),
            };
            (!ahead_only || along > 0).then(|| (along.abs() + across.abs() * 2, candidate))
        })
        .min_by_key(|&(distance, _)| distance)
        .map(|(_, candidate)| candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_counts_double_and_straying_sideways_costs_double_again() {
        let candidates = [("below", (0, 2)), ("right", (3, 0)), ("behind", (-1, 0))];
        assert_eq!(
            toward((0, 0), PanDirection::Right, true, candidates),
            Some("right")
        );
        assert_eq!(
            toward((0, 0), PanDirection::Down, true, candidates),
            Some("below")
        );
        assert_eq!(
            toward((0, 0), PanDirection::Left, true, candidates),
            Some("behind")
        );
        assert_eq!(toward((0, 0), PanDirection::Up, true, candidates), None);
        assert_eq!(
            toward((0, 0), PanDirection::Up, false, candidates),
            Some("behind"),
            "with nothing picked, the nearest on any side"
        );
    }
}
