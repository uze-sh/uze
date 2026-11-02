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

/// Declares an [`Earlier`] entry, filling its `site` with the file it is
/// written in.
#[macro_export]
macro_rules! earlier {
    ($name:expr, $lasts:expr $(, spelled: [$($spelled:expr),* $(,)?])? $(,)?) => {
        $crate::earlier::Earlier {
            name: $name,
            lasts: $lasts,
            spelled: &[$($($spelled),*)?],
            site: file!(),
        }
    };
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

    #[test]
    fn an_entry_names_the_file_that_declares_it() {
        let entry = crate::earlier!("x", Lasts::Unbounded, spelled: ["old.json"]);
        assert_eq!(entry.site, file!());
        assert_eq!(entry.spelled, &["old.json"]);
    }
}
