//! Type-level authentication states.
//!
//! Uses a sealed trait pattern so that `AuthState` can only be
//! `Authenticated` or `Unauthenticated` — no third-party impls.

mod sealed {
    pub trait Sealed {}
}

/// Marker trait for authentication states.
pub trait AuthState: sealed::Sealed + Send + Sync + 'static {}

/// The client has no API key — only public endpoints are accessible.
#[derive(Debug, Clone, Copy)]
pub struct Unauthenticated;

/// The client has a valid API key — all endpoints are accessible.
#[derive(Debug, Clone, Copy)]
pub struct Authenticated;

impl sealed::Sealed for Unauthenticated {}
impl sealed::Sealed for Authenticated {}
impl AuthState for Unauthenticated {}
impl AuthState for Authenticated {}
