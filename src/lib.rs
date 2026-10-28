//! The catalogues of spatial data infrastructures (IDE), read into one list of services.
//!
//! Every IDE publishes its catalogue its own way (IDECA a flat list, Barranquilla themes holding
//! maps, IGAC a DataTables answer). Each plugin turns its catalogue into [`Entry`]s, and this
//! crate does the rest the same way for all of them:
//!
//! - [`entry::Entry`]: one service, with only `id`, `name` and `url` required.
//! - [`kind::Kind`]: what a service is, read from its address (catalogues mislabel them).
//! - [`url`]: an ArcGIS address split into its layer and the `where` a catalogue put in it.
//! - [`index`]: the list as the plugin embeds it, and how it stands against the live
//!   catalogue.
//! - [`browse`]: the list as the panel walks it, category by category.
//! - [`arcgis`]: the addresses RustGIS reads a service's features or map image from.
//!
//! Catalogues are input from the web: nothing here panics on them, and [`entry::clean`] drops
//! what can't be used.

#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable, clippy::todo, clippy::indexing_slicing)
)]
#![forbid(unsafe_code)]

pub mod arcgis;
pub mod browse;
pub mod entry;
pub mod index;
pub mod kind;
pub mod url;

pub use entry::{Entry, clean};
pub use kind::Kind;
