/// Define a typed resource ID newtype.
///
/// # Examples
/// ```rust,ignore
/// resource_id!(ItemId, u32);
/// resource_id!(/// A GW2 achievement group ID (UUID). AchievementGroupId, String);
/// ```
#[macro_export]
macro_rules! resource_id {
    ($(#[$meta:meta])* $name:ident, $inner:ty) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, ::serde::Serialize, ::serde::Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub $inner);

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<$inner> for $name {
            fn from(id: $inner) -> Self { Self(id) }
        }

        impl From<$name> for $inner {
            fn from(id: $name) -> Self { id.0 }
        }

        impl $crate::resource::ResourceId for $name {}
    };
}
