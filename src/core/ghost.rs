// core/ghost.rs
use crate::core::base::Base;

pub trait Ghost: Sized + Base {
    const URL: &'static str;
}

#[macro_export]
macro_rules! ghost {
    ($ty:ty, $url:expr) => {
        impl $crate::core::ghost::Ghost for $ty {
            const URL: &'static str = $url;
        }
    };
}
