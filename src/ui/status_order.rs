//! The order of the rows declared extensions draw in the workspace
//! sidebar's header: the operator's, from the Extensions screen, kept in
//! the client's layout (`ManagementLayout::status_order`).
//!
//! The order names extensions by the key each is switched by. One it does
//! not name stands after those it does, in the order the extensions were
//! found, so an extension installed since it was last set needs no answer.

/// `found`, in discovery order, arranged by `order`.
pub(crate) fn arranged<'a>(
    order: &[String],
    found: impl IntoIterator<Item = &'a str>,
) -> Vec<&'a str> {
    let mut found: Vec<(usize, &'a str)> = found.into_iter().enumerate().collect();
    found.sort_by_key(|(discovered, key)| {
        let named = order.iter().position(|named| named == key);
        (named.is_none(), named.unwrap_or(*discovered), *discovered)
    });
    found.into_iter().map(|(_, key)| key).collect()
}

/// The order after `key` moves one place, up when `up`: the whole
/// arrangement, so what was found but never named is named from now on.
/// `None` when it cannot move that way, or is not among `found`.
pub(crate) fn moved<'a>(
    order: &[String],
    found: impl IntoIterator<Item = &'a str>,
    key: &str,
    up: bool,
) -> Option<Vec<String>> {
    let mut arranged = arranged(order, found);
    let at = arranged.iter().position(|candidate| *candidate == key)?;
    let to = if up {
        at.checked_sub(1)?
    } else {
        Some(at + 1).filter(|to| *to < arranged.len())?
    };
    arranged.swap(at, to);
    Some(arranged.into_iter().map(str::to_owned).collect())
}

/// Whether `key` can move up, and down, among `found` arranged by `order`.
pub(crate) fn can_move<'a>(
    order: &[String],
    found: impl IntoIterator<Item = &'a str>,
    key: &str,
) -> (bool, bool) {
    let arranged = arranged(order, found);
    match arranged.iter().position(|candidate| *candidate == key) {
        Some(at) => (at > 0, at + 1 < arranged.len()),
        None => (false, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(keys: &[&str]) -> Vec<String> {
        keys.iter().map(|key| (*key).to_owned()).collect()
    }

    #[test]
    fn an_absent_order_is_the_order_of_discovery() {
        assert_eq!(arranged(&[], ["a", "b", "c"]), ["a", "b", "c"]);
    }

    #[test]
    fn what_the_order_names_stands_first_and_the_rest_keeps_discovery_order() {
        assert_eq!(
            arranged(&order(&["c", "gone", "a"]), ["a", "b", "c", "d"]),
            ["c", "a", "b", "d"]
        );
    }

    #[test]
    fn moving_swaps_with_the_neighbour_and_names_everything() {
        assert_eq!(
            moved(&[], ["a", "b", "c"], "b", true),
            Some(order(&["b", "a", "c"]))
        );
        assert_eq!(
            moved(&order(&["b", "a"]), ["a", "b", "c"], "a", false),
            Some(order(&["b", "c", "a"]))
        );
        assert_eq!(moved(&[], ["a", "b"], "a", true), None, "already first");
        assert_eq!(moved(&[], ["a", "b"], "b", false), None, "already last");
        assert_eq!(moved(&[], ["a"], "z", true), None, "not found");
    }

    #[test]
    fn the_ends_cannot_move_past_themselves() {
        assert_eq!(can_move(&[], ["a", "b", "c"], "a"), (false, true));
        assert_eq!(can_move(&[], ["a", "b", "c"], "b"), (true, true));
        assert_eq!(can_move(&[], ["a"], "a"), (false, false));
    }
}
