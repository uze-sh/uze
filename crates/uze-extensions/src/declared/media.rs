//! The pictures a declared extension's views name: read through the host,
//! decoded on the pool one at a time, and held under the extension's
//! budget (`extension-media-and-resident` M2, M3; R-S12).
//!
//! A view binds against what is held and never waits: a picture not yet
//! read is drawn as its `alt`, one refused as the bound it broke, and the
//! build that named it asks for it, so the next job reads it. One read is
//! out at a time, so an extension naming a hundred images decodes them one
//! after another rather than a hundred at once.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Instant;

use uze_contract::{Identity, SpriteDecl};

use crate::contract::{Job, Opaque};
use crate::shared::picture::{self, Refused};
use crate::view::Picture;

pub use uze_contract::IMAGE_FILE_MAX;

/// The decoded bytes (RGBA) an extension's pictures may hold at once. A
/// decode whose header asks past what is left is refused before its
/// pixels are allocated.
pub const MEDIA_BYTES_MAX: usize = 32 * 1024 * 1024;

/// What `scratch:` names: a file the extension's program wrote.
const SCRATCH: &str = uze_contract::SCRATCH_PREFIX;

/// Something a view names that the host reads: an `Image`'s file, or a
/// declared sprite's sheet.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum MediaKey {
    Image(String),
    Sheet(String),
}

/// What reading one held.
#[derive(Clone, Debug)]
pub(super) enum Held {
    Picture(Picture),
    /// A sheet cut into its frames, left to right and top to bottom.
    Frames(Vec<Picture>),
    /// Why there is none, as the surface says it.
    Refused(String),
}

impl Held {
    fn bytes(&self) -> usize {
        let of = |picture: &Picture| picture.pixels.len() * 4;
        match self {
            Self::Picture(picture) => of(picture),
            Self::Frames(frames) => frames.iter().map(of).sum(),
            Self::Refused(_) => 0,
        }
    }
}

/// A sprite's state as a build found it, kept so the next build of the
/// same state keeps playing from where it began.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Playing {
    pub state: String,
    pub since: Option<Instant>,
}

/// An extension's pictures.
#[derive(Debug, Default)]
pub(super) struct Shelf {
    held: BTreeMap<MediaKey, Held>,
    /// Held ones to read again, still drawn until their new read lands: a
    /// program's scratch, after its source answered again.
    stale: BTreeSet<MediaKey>,
    /// The read out, at most one.
    reading: Option<MediaKey>,
    /// Bumped by [`Shelf::clear`] and carried by each read, so a read
    /// asked before a clear and landing after a new read of the same key
    /// started is never taken as the new one's.
    epoch: u64,
    /// Bumped whenever what is held changes, so a build of the old shelf
    /// is never served.
    pub generation: u64,
}

/// What a read answered, to the shelf that asked.
pub(super) struct MediaAnswer {
    key: MediaKey,
    held: Held,
    epoch: u64,
}

/// What a build reads off the shelf.
#[derive(Clone, Copy)]
pub(super) struct Lookup<'a> {
    pub shelf: &'a Shelf,
    pub sprites: &'a [SpriteDecl],
    /// The icons it declares beyond the host's.
    pub icons: &'a [uze_contract::IconDecl],
    /// The sprites the last build of the same surface was playing.
    pub before: Option<&'a BTreeMap<String, Playing>>,
    /// The host's clock at its last pass: when a state seen now began.
    pub now: Option<Instant>,
}

impl Shelf {
    pub fn get(&self, key: &MediaKey) -> Option<&Held> {
        self.held.get(key)
    }

    /// Forgets everything, as a new compile does.
    pub fn clear(&mut self) {
        self.held.clear();
        self.stale.clear();
        self.reading = None;
        self.epoch += 1;
        self.generation += 1;
    }

    /// Marks every picture a program wrote to be read again, once its
    /// source answered: the file it names may hold a new picture.
    pub fn stale_scratch(&mut self) {
        let scratch = self
            .held
            .keys()
            .filter(|key| matches!(key, MediaKey::Image(name) if name.starts_with(SCRATCH)));
        self.stale.extend(scratch.cloned());
    }

    /// The read to hand the host now: the first of `wanted` not held, or
    /// held and stale, while none is out. What no build wants any more is
    /// let go first, so the budget is what the surfaces show.
    pub fn next(
        &mut self,
        wanted: &BTreeSet<MediaKey>,
        identity: &Identity,
        sprites: &[SpriteDecl],
    ) -> Option<Job> {
        if self.reading.is_some() {
            return None;
        }
        self.held.retain(|key, _| wanted.contains(key));
        self.stale.retain(|key| wanted.contains(key));
        let key = loop {
            let key = wanted
                .iter()
                .find(|key| !self.held.contains_key(*key) || self.stale.contains(*key))?
                .clone();
            match refused_name(&key, sprites) {
                Some(why) => {
                    self.stale.remove(&key);
                    self.held.insert(key, Held::Refused(why));
                    self.generation += 1;
                }
                None => break key,
            }
        };
        let room = MEDIA_BYTES_MAX.saturating_sub(
            self.held
                .iter()
                .filter(|(held, _)| **held != key)
                .map(|(_, held)| held.bytes())
                .sum(),
        );
        self.reading = Some(key.clone());
        Some(read(identity.clone(), key, sprites, room, self.epoch))
    }

    /// Takes a read's answer; one for a read no longer out is dropped.
    pub fn absorb(&mut self, answer: MediaAnswer) {
        if answer.epoch != self.epoch || self.reading.as_ref() != Some(&answer.key) {
            return;
        }
        self.reading = None;
        self.stale.remove(&answer.key);
        self.held.insert(answer.key, answer.held);
        self.generation += 1;
    }
}

/// Why `key` is not read at all: a name that leaves the extension or its
/// scratch, a format it does not show, or a sprite it does not declare.
fn refused_name(key: &MediaKey, sprites: &[SpriteDecl]) -> Option<String> {
    let name = match key {
        MediaKey::Image(name) => name,
        MediaKey::Sheet(id) => {
            return match sprites.iter().find(|sprite| sprite.id == *id) {
                Some(sprite) => refused_path(&sprite.sheet),
                None => Some(format!("it declares no sprite `{id}`")),
            };
        }
    };
    match name.strip_prefix(SCRATCH) {
        Some(file) => {
            let plain =
                !file.is_empty() && !file.contains(['/', '\\']) && file != "." && file != "..";
            match plain {
                true => refused_format(file),
                false => Some(format!("`{name}` names no file of its scratch")),
            }
        }
        None => refused_path(name),
    }
}

fn refused_path(path: &str) -> Option<String> {
    let inside = !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && path.split('/').all(|part| part != "..");
    match inside {
        true => refused_format(path),
        false => Some(format!("`{path}` is not a file of the extension")),
    }
}

fn refused_format(file: &str) -> Option<String> {
    let extension = Path::new(file)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("png" | "jpg" | "jpeg" | "gif") => None,
        _ => Some(format!("`{file}` is not a PNG, JPEG or GIF")),
    }
}

/// The job that reads `key` through the host and decodes it within `room`
/// bytes, cutting a sheet into its frames.
fn read(identity: Identity, key: MediaKey, sprites: &[SpriteDecl], room: usize, epoch: u64) -> Job {
    let sprite = match &key {
        MediaKey::Sheet(id) => sprites.iter().find(|sprite| sprite.id == *id).cloned(),
        MediaKey::Image(_) => None,
    };
    let fallback_key = key.clone();
    Job {
        run: Box::new(move |host| {
            let name = match (&key, &sprite) {
                (MediaKey::Image(name), _) => name.clone(),
                (MediaKey::Sheet(_), Some(sprite)) => sprite.sheet.clone(),
                (MediaKey::Sheet(id), None) => id.clone(),
            };
            let held = match host.media(&identity, &name) {
                Ok(bytes) => decoded(&name, &bytes, room, sprite.as_ref()),
                Err(why) => Held::Refused(why),
            };
            Opaque::new(MediaAnswer { key, held, epoch })
        }),
        fallback: Box::new(move || {
            Opaque::new(MediaAnswer {
                key: fallback_key,
                held: Held::Refused("the picture could not be read".to_owned()),
                epoch,
            })
        }),
    }
}

/// `bytes` read as the picture `name` names, within `room` decoded bytes:
/// refused past [`IMAGE_FILE_MAX`] before decoding.
pub(super) fn decoded(name: &str, bytes: &[u8], room: usize, sprite: Option<&SpriteDecl>) -> Held {
    if bytes.len() > IMAGE_FILE_MAX {
        return Held::Refused(format!(
            "the picture is over {} MiB",
            IMAGE_FILE_MAX / (1024 * 1024)
        ));
    }
    let file = name.strip_prefix(SCRATCH).unwrap_or(name);
    let decoded = match picture::decode_within(Path::new(file), bytes, room) {
        Ok(decoded) => decoded,
        Err(Refused::PastBound) => {
            return Held::Refused(format!(
                "its pictures would hold over {} MiB",
                MEDIA_BYTES_MAX / (1024 * 1024)
            ));
        }
        Err(Refused::NotAnImage) => {
            return Held::Refused(format!("`{file}` is not a picture it can read"));
        }
    };
    match sprite {
        None => Held::Picture(decoded),
        Some(sprite) => match frames(&decoded, sprite) {
            Some(frames) => Held::Frames(frames),
            None => Held::Refused(format!(
                "`{}` does not divide into its frames",
                sprite.sheet
            )),
        },
    }
}

/// `sheet` cut into `sprite`'s frames, row by row. `None` when its size
/// is not whole frames, as a sheet shrunk on decoding is not.
fn frames(sheet: &Picture, sprite: &SpriteDecl) -> Option<Vec<Picture>> {
    let (width, height) = (
        usize::try_from(sprite.frame.0).ok()?,
        usize::try_from(sprite.frame.1).ok()?,
    );
    let columns = usize::try_from(sprite.columns).ok()?;
    if width == 0
        || height == 0
        || columns == 0
        || !sheet.width.is_multiple_of(width)
        || !sheet.height.is_multiple_of(height)
        || columns > sheet.width / width
    {
        return None;
    }
    let rows = sheet.height / height;
    let mut cut = Vec::with_capacity(columns * rows);
    for row in 0..rows {
        for column in 0..columns {
            let mut pixels = Vec::with_capacity(width * height);
            for y in row * height..(row + 1) * height {
                let start = y * sheet.width + column * width;
                pixels.extend_from_slice(&sheet.pixels[start..start + width]);
            }
            cut.push(Picture {
                width,
                height,
                pixels: pixels.into(),
            });
        }
    }
    Some(cut)
}

#[cfg(test)]
mod tests {
    use uze_contract::SpriteState;

    use super::*;

    fn png(width: u32, height: u32, samples: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(samples).unwrap();
        writer.finish().unwrap();
        bytes
    }

    fn sprite() -> SpriteDecl {
        SpriteDecl {
            id: "pet".to_owned(),
            sheet: "pet.png".to_owned(),
            frame: (1, 1),
            columns: 2,
            states: vec![SpriteState {
                name: "idle".to_owned(),
                frames: vec![0, 1],
                fps: 2,
                looped: true,
            }],
            fallback: "idle".to_owned(),
        }
    }

    #[test]
    fn a_file_over_its_bound_is_refused_before_it_is_decoded() {
        let over = vec![0u8; IMAGE_FILE_MAX + 1];
        let Held::Refused(why) = decoded("big.png", &over, usize::MAX, None) else {
            panic!("decoded past the file bound");
        };
        assert_eq!(why, "the picture is over 8 MiB");
    }

    #[test]
    fn a_decode_past_what_the_extension_has_left_is_refused_naming_the_bound() {
        let image = png(2, 2, &[9; 16]);
        let Held::Refused(why) = decoded("a.png", &image, 15, None) else {
            panic!("decoded past the pixel bound");
        };
        assert_eq!(why, "its pictures would hold over 32 MiB");
        assert!(matches!(
            decoded("scratch:a.png", &image, 16, None),
            Held::Picture(_)
        ));
    }

    #[test]
    fn a_sheet_is_cut_into_its_frames_left_to_right() {
        let sheet = png(2, 1, &[255, 0, 0, 255, 0, 0, 255, 255]);
        let Held::Frames(frames) = decoded("pet.png", &sheet, usize::MAX, Some(&sprite())) else {
            panic!("not cut");
        };
        assert_eq!(frames.len(), 2);
        assert_eq!(*frames[0].pixels, [[255, 0, 0, 255]]);
        assert_eq!(*frames[1].pixels, [[0, 0, 255, 255]]);
    }

    #[test]
    fn a_name_leaving_the_extension_or_its_scratch_is_never_read() {
        let refused = |name: &str| refused_name(&MediaKey::Image(name.to_owned()), &[]);
        assert_eq!(refused("art/cat.png"), None);
        assert_eq!(refused("scratch:shot.png"), None);
        for name in [
            "../cat.png",
            "/etc/cat.png",
            "scratch:../x.png",
            "scratch:a/b.png",
        ] {
            assert!(refused(name).is_some(), "{name}");
        }
        assert_eq!(
            refused("notes.txt").as_deref(),
            Some("`notes.txt` is not a PNG, JPEG or GIF")
        );
        assert!(refused_name(&MediaKey::Sheet("ghost".to_owned()), &[]).is_some());
    }

    #[test]
    fn one_read_is_out_at_a_time_and_what_no_build_wants_is_let_go() {
        let identity = Identity {
            marketplace: "m".to_owned(),
            plugin: "p".to_owned(),
            id: "e".to_owned(),
        };
        let mut shelf = Shelf::default();
        let wanted: BTreeSet<MediaKey> = ["a.png", "b.png"]
            .into_iter()
            .map(|name| MediaKey::Image(name.to_owned()))
            .collect();
        assert!(shelf.next(&wanted, &identity, &[]).is_some());
        assert!(shelf.next(&wanted, &identity, &[]).is_none(), "one out");
        shelf.absorb(MediaAnswer {
            key: MediaKey::Image("a.png".to_owned()),
            held: Held::Refused("gone".to_owned()),
            epoch: 0,
        });
        assert!(shelf.get(&MediaKey::Image("a.png".to_owned())).is_some());
        let only_b: BTreeSet<MediaKey> = [MediaKey::Image("b.png".to_owned())].into();
        assert!(shelf.next(&only_b, &identity, &[]).is_some(), "b next");
        assert!(
            shelf.get(&MediaKey::Image("a.png".to_owned())).is_none(),
            "a let go"
        );
    }
}
