//! Client-side corrections for known permanent GW2 API bugs.
//!
//! The GW2 API has a small number of well-documented, long-standing bugs where
//! the API consistently returns incorrect data. Rather than surfacing corrupt
//! data to callers, this module applies corrections transparently after
//! deserialization, via the [`Patchable`][crate::resource::Patchable] trait.
//!
//! Each sub-module documents its bug source and correction table.

pub mod minis;
