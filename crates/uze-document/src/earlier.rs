//! What UZE does only because an earlier release left something behind,
//! declared so that it can be found, bounded and retired.
//!
//! Every such piece of support — a rung of a record's ladder, a path an
//! earlier release wrote, a file an integration once delivered, an older
//! wire or command spelling — is an [`Earlier`] entry in the registry of its
//! kind, and names how long it [`Lasts`]. [`FLOOR`] is the oldest release a
//! machine can come from and still be carried across: raising it makes the
//! architecture suite name every entry below it, and deleting those entries
//! is the whole of retiring that release's support.
//!
//! An entry lasts *until* a release only when every run since that release
//! carries out what it handles. A record rewritten only when it is next
//! changed can sit in its old shape on a machine indefinitely, so the rung
//! that reads it is [`Lasts::Unbounded`], and the floor never passes it.

/// A published release of UZE, ordered the way releases were cut: every
/// alpha, then every beta, then the stable release of the same number.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Release {
    major: u32,
    minor: u32,
    patch: u32,
    stage: Stage,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum Stage {
    Alpha(u32),
    Beta(u32),
    Stable,
}

impl Release {
    pub const fn alpha(major: u32, minor: u32, patch: u32, number: u32) -> Self {
        Self::at(major, minor, patch, Stage::Alpha(number))
    }

    pub const fn beta(major: u32, minor: u32, patch: u32, number: u32) -> Self {
        Self::at(major, minor, patch, Stage::Beta(number))
    }

    pub const fn stable(major: u32, minor: u32, patch: u32) -> Self {
        Self::at(major, minor, patch, Stage::Stable)
    }

    const fn at(major: u32, minor: u32, patch: u32, stage: Stage) -> Self {
        Self {
            major,
            minor,
            patch,
            stage,
        }
    }

    /// A release as a tag or a package version spells it: `1.0.0-beta.22`,
    /// with or without a leading `v`.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.strip_prefix('v').unwrap_or(text);
        let (number, pre) = match text.split_once('-') {
            Some((number, pre)) => (number, Some(pre)),
            None => (text, None),
        };
        let mut parts = number.split('.').map(str::parse::<u32>);
        let (major, minor, patch) = (
            parts.next()?.ok()?,
            parts.next()?.ok()?,
            parts.next()?.ok()?,
        );
        if parts.next().is_some() {
            return None;
        }
        let stage = match pre {
            None => Stage::Stable,
            Some(pre) => match pre.split_once('.')? {
                ("alpha", n) => Stage::Alpha(n.parse().ok()?),
                ("beta", n) => Stage::Beta(n.parse().ok()?),
                _ => return None,
            },
        };
        Some(Self::at(major, minor, patch, stage))
    }
}

impl std::fmt::Display for Release {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)?;
        match self.stage {
            Stage::Alpha(n) => write!(formatter, "-alpha.{n}"),
            Stage::Beta(n) => write!(formatter, "-beta.{n}"),
            Stage::Stable => Ok(()),
        }
    }
}

/// The oldest release a machine can come from and still be carried across.
pub const FLOOR: Release = Release::beta(1, 0, 0, 22);

/// How long a piece of support for an earlier release is needed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Lasts {
    /// Until the floor passes this release: the last one whose run could
    /// still leave what the entry handles on a machine.
    Until(Release),
    /// For as long as what carries it out does not run on every run: a
    /// record rewritten only when it is next changed, a leftover removed
    /// only when its package is next attached.
    Unbounded,
}

impl Lasts {
    /// Whether the floor has passed it, so the entry is due for deletion.
    pub fn below(self, floor: Release) -> bool {
        matches!(self, Lasts::Until(release) if release < floor)
    }
}

/// One piece of support for an earlier release.
#[derive(Clone, Copy, Debug)]
pub struct Earlier {
    /// A stable name for it, printed when the floor passes it.
    pub name: &'static str,
    pub lasts: Lasts,
    /// The literals only this entry may spell in production code: the
    /// retired paths, file names and names it handles.
    pub spelled: &'static [&'static str],
    /// The file that declares it, which is where it is deleted.
    pub site: &'static str,
}

/// The rungs of `T`'s ladder as entries, for the crate that owns `T` to
/// list among its own.
///
/// # Panics
///
/// When the ladder is not contiguous, does not end at the shape `T`
/// writes, or a rung lasts less than the one beneath it: a ladder the
/// architecture suite gathers is a ladder this checks.
pub fn rungs<T: crate::Shaped>(site: &'static str) -> Vec<Earlier> {
    let ladder = T::ladder();
    for (index, step) in ladder.iter().enumerate() {
        assert_eq!(
            step.to,
            step.from + 1,
            "{}: rung {index} skips a shape",
            T::KIND
        );
        if let Some(next) = ladder.get(index + 1) {
            assert_eq!(
                next.from,
                step.to,
                "{}: rungs {index} and {} do not meet",
                T::KIND,
                index + 1
            );
            assert!(
                !outlasts(step.lasts, next.lasts),
                "{}: rung {index} lasts longer than the rung above it",
                T::KIND
            );
        }
    }
    if let Some(top) = ladder.last() {
        assert_eq!(
            top.to,
            T::SHAPE,
            "{}: the ladder stops below the shape written",
            T::KIND
        );
    }
    ladder
        .iter()
        .map(|step| Earlier {
            name: T::KIND,
            lasts: step.lasts,
            spelled: &[],
            site,
        })
        .collect()
}

/// Whether `lower` lasts past `upper`: a record can only be met in a lower
/// shape for as long as it can be met in every shape above it.
fn outlasts(lower: Lasts, upper: Lasts) -> bool {
    match (lower, upper) {
        (Lasts::Unbounded, Lasts::Until(_)) => true,
        (Lasts::Until(lower), Lasts::Until(upper)) => lower > upper,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn releases_order_as_they_were_cut() {
        let alpha = Release::parse("v0.0.0-alpha.10").unwrap();
        let early_beta = Release::parse("1.0.0-beta.9").unwrap();
        let late_beta = Release::parse("1.0.0-beta.22").unwrap();
        let stable = Release::parse("1.0.0").unwrap();
        assert!(alpha < early_beta && early_beta < late_beta && late_beta < stable);
        assert_eq!(late_beta, Release::beta(1, 0, 0, 22));
        assert_eq!(late_beta.to_string(), "1.0.0-beta.22");
    }

    #[test]
    fn what_is_not_a_release_is_refused() {
        for text in ["1.0", "1.0.0-rc.1", "1.0.0-beta", "1.0.0.0", "x.0.0"] {
            assert_eq!(Release::parse(text), None, "{text}");
        }
    }

    #[test]
    fn only_a_bounded_entry_falls_below_the_floor() {
        let floor = Release::beta(1, 0, 0, 22);
        assert!(Lasts::Until(Release::beta(1, 0, 0, 21)).below(floor));
        assert!(!Lasts::Until(floor).below(floor));
        assert!(!Lasts::Unbounded.below(floor));
    }

    const fn step(from: u32, to: u32, lasts: Lasts) -> crate::Step {
        crate::Step {
            from,
            to,
            climb: Ok,
            lasts,
        }
    }

    const fn until(number: u32) -> Lasts {
        Lasts::Until(Release::beta(1, 0, 0, number))
    }

    macro_rules! record {
        ($name:ident, shape $shape:literal, $($step:expr),+) => {
            #[derive(serde::Deserialize)]
            struct $name;

            impl crate::Shaped for $name {
                const SHAPE: u32 = $shape;
                const KIND: &'static str = stringify!($name);

                fn ladder() -> crate::Ladder {
                    const LADDER: crate::Ladder = &[$($step),+];
                    LADDER
                }
            }
        };
    }

    /// The release floor can only judge the rungs it is handed: a record whose
    /// entries went missing would pass that check with nothing checked.
    #[test]
    fn every_rung_is_an_entry_naming_its_record_and_how_long_it_lasts() {
        record!(Listed, shape 3, step(1, 2, until(5)), step(2, 3, Lasts::Unbounded));

        let entries = rungs::<Listed>("declared/here.rs");

        let lasts: Vec<Lasts> = entries.iter().map(|entry| entry.lasts).collect();
        assert_eq!(lasts, [until(5), Lasts::Unbounded]);
        for entry in entries {
            assert_eq!(entry.name, "Listed");
            assert_eq!(entry.site, "declared/here.rs");
        }
    }

    #[test]
    #[should_panic(expected = "rung 0 skips a shape")]
    fn a_rung_that_skips_a_shape_is_refused() {
        record!(Skipping, shape 3, step(1, 3, Lasts::Unbounded));
        rungs::<Skipping>("here.rs");
    }

    #[test]
    #[should_panic(expected = "rungs 0 and 1 do not meet")]
    fn rungs_that_do_not_meet_are_refused() {
        record!(Apart, shape 4, step(1, 2, Lasts::Unbounded), step(3, 4, Lasts::Unbounded));
        rungs::<Apart>("here.rs");
    }

    #[test]
    #[should_panic(expected = "the ladder stops below the shape written")]
    fn a_ladder_short_of_the_shape_written_is_refused() {
        record!(Short, shape 3, step(1, 2, Lasts::Unbounded));
        rungs::<Short>("here.rs");
    }

    #[test]
    #[should_panic(expected = "rung 0 lasts longer than the rung above it")]
    fn a_rung_outlasting_a_later_release_above_it_is_refused() {
        record!(Late, shape 3, step(1, 2, until(9)), step(2, 3, until(5)));
        rungs::<Late>("here.rs");
    }

    #[test]
    #[should_panic(expected = "rung 0 lasts longer than the rung above it")]
    fn an_unbounded_rung_beneath_a_bounded_one_is_refused() {
        record!(Open, shape 3, step(1, 2, Lasts::Unbounded), step(2, 3, until(5)));
        rungs::<Open>("here.rs");
    }

    /// A record met in a lower shape can be met in every shape above it, so
    /// each rung may last as long as the next one and no longer.
    #[test]
    fn a_rung_may_last_as_long_as_the_one_above_it() {
        record!(Equal, shape 3, step(1, 2, until(5)), step(2, 3, until(5)));
        record!(Rising, shape 3, step(1, 2, until(5)), step(2, 3, until(9)));
        record!(Bounded, shape 3, step(1, 2, until(5)), step(2, 3, Lasts::Unbounded));
        record!(Open, shape 3, step(1, 2, Lasts::Unbounded), step(2, 3, Lasts::Unbounded));

        assert_eq!(rungs::<Equal>("here.rs").len(), 2);
        assert_eq!(rungs::<Rising>("here.rs").len(), 2);
        assert_eq!(rungs::<Bounded>("here.rs").len(), 2);
        assert_eq!(rungs::<Open>("here.rs").len(), 2);
    }
}
