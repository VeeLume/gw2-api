// core/ghost_id.rs
use crate::core::base::Base;
use std::fmt;

pub trait Identifiable {
    type Id: Clone + Eq + std::hash::Hash + fmt::Display + Send + Sync + 'static;
    fn id(&self) -> &Self::Id;
}

pub trait GhostId: Sized + Base + Identifiable {
    const URL: &'static str;

    #[inline]
    fn url_for_id(id: &Self::Id) -> String {
        format!("{}/{}", Self::URL, id)
    }
}

// macro: implement GhostId + Identifiable (no inherent network methods)
#[macro_export]
macro_rules! ghost_id {
    // full form
    ($ty:ty, $url:expr, $id_ty:ty, $field:ident) => {
        impl $crate::core::ghost_id::GhostId for $ty {
            const URL: &'static str = $url;
        }
        impl $crate::core::ghost_id::Identifiable for $ty {
            type Id = $id_ty;
            fn id(&self) -> &Self::Id {
                &self.$field
            }
        }
    };
    // convenience: field named `id`
    ($ty:ty, $url:expr, $id_ty:ty) => {
        $crate::ghost_id!($ty, $url, $id_ty, id);
    };
}

/// Create a typed `...Id` newtype for a `GhostId` model that can fetch itself.
/// Usage: `id_type!(MountSkin => MountSkinId);`
/// The inner id type is inferred from `<MountSkin as Identifiable>::Id`.
#[macro_export]
macro_rules! id_type {
    ($owner:ty => $name:ident) => {
        #[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[serde(transparent)]
        pub struct $name(
            pub < $owner as $crate::core::ghost_id::Identifiable >::Id
        );

        impl $name {
            #[inline]
            pub fn id(&self) -> &< $owner as $crate::core::ghost_id::Identifiable >::Id {
                &self.0
            }

            #[inline]
            pub fn into_inner(self) -> < $owner as $crate::core::ghost_id::Identifiable >::Id {
                self.0
            }

            /// Fetch the referenced owner using the owner's cache.
            /// Requires that the owner used `cached_resource!`.
            pub async fn get(&self, force: bool)
                -> Result<std::sync::Arc<$owner>, $crate::api::client::ApiError>
            {
                <$owner>::get(self.0.clone(), force).await
            }
        }

        // Display / Debug just forward to the inner id
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                std::fmt::Display::fmt(&self.0, f)
            }
        }
        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                std::fmt::Debug::fmt(&self.0, f)
            }
        }

        // Convenience conversions
        impl From<< $owner as $crate::core::ghost_id::Identifiable >::Id> for $name {
            fn from(v: < $owner as $crate::core::ghost_id::Identifiable >::Id) -> Self { Self(v) }
        }
        impl From<$name> for < $owner as $crate::core::ghost_id::Identifiable >::Id {
            fn from(v: $name) -> Self { v.0 }
        }

        // AsRef / Borrow make it easy to pass to APIs expecting the raw id type
        impl AsRef<< $owner as $crate::core::ghost_id::Identifiable >::Id> for $name {
            fn as_ref(&self) -> &< $owner as $crate::core::ghost_id::Identifiable >::Id { &self.0 }
        }
        impl std::borrow::Borrow<< $owner as $crate::core::ghost_id::Identifiable >::Id> for $name {
            fn borrow(&self) -> &< $owner as $crate::core::ghost_id::Identifiable >::Id { &self.0 }
        }

        // Default if inner supports it
        impl Default for $name
        where
            < $owner as $crate::core::ghost_id::Identifiable >::Id: Default
        {
            fn default() -> Self { Self(Default::default()) }
        }
    };
}
