//! Tests for the management client, one file per screen or concern.

// Gated again so the architecture suite, which reads `#[cfg(test)] mod`
// declarations, knows these files are test code.
#[cfg(test)]
mod extensions;
#[cfg(test)]
mod harnesses;
#[cfg(test)]
mod keys;
#[cfg(test)]
mod layout;
#[cfg(test)]
mod marketplaces;
#[cfg(test)]
mod navigation;
#[cfg(test)]
mod overlays;
#[cfg(test)]
mod overview;
#[cfg(test)]
mod plugins;
#[cfg(test)]
mod profiles;
#[cfg(test)]
mod search;
#[cfg(test)]
mod session;
#[cfg(test)]
mod settings;
#[cfg(test)]
mod support;
