use serde::{ Serialize, Deserialize };
use crate::error::Error;
use std::time::Duration;
#[cfg(feature = "duration_checked_float")]
use std::time::FromSecsError;

pub trait IntoClones<Tuple>: Clone {
    fn into_clones(self) -> Tuple;
}

macro_rules! impl_into_clones {
    // [@clones(self) T (...)] => [(... self,)]
    [@clones($self:ident) $h:ident ($($code:tt)*)] => { ($($code)* $self,) };
    // [@clones(self) T T ... T (...)] => [@clones(self) T ... T (... self.clone(),)]
    [@clones($self:ident) $h:ident $($t:ident)+ ($($code:tt)*)] => {
        impl_into_clones![
            @clones($self) $($t)+ ($($code)* $self.clone(),)
        ]
    };
    {$h:ident $($t:ident)*} => {
        impl<$h: Clone> IntoClones<($h $(, $t)*,)> for $h {
            fn into_clones(self) -> ($h $(, $t)*,) {
                // [@clones(self) T T ... T ()]
                impl_into_clones![@clones(self) $h $($t)* ()]
            }
        }
        impl_into_clones!{$($t)*}
    };
    () => {};
}

impl_into_clones!{T T T T T T T T T T T}

#[macro_export]
macro_rules! tuple_from_box_api {
    ($f:path [ $($args:expr),* , @out ]) => {
        {
            let mut result = ::std::mem::MaybeUninit::uninit();
            unsafe {
                if $f($($args),*, result.as_mut_ptr()) < 0 {
                    return Err($crate::error::TarantoolError::last().into());
                }
                Ok($crate::tuple::Tuple::try_from_ptr(result.assume_init()))
            }
        }
    }
}

#[inline]
pub fn rmp_to_vec<T>(val: &T) -> Result<Vec<u8>, Error>
    where
        T: Serialize + ?Sized
{
    Ok(rmp_serde::to_vec(val)?)
}

#[derive(Clone, Debug, Serialize, Deserialize, tlua::Push)]
pub enum NumOrStr {
    Num(u32),
    // TODO(gmoshkin): this should be a `&str` instead, but
    // `#[derive(tlua::Push)]` doesn't support generic parameters yet
    Str(String),
}

impl From<u32> for NumOrStr {
    #[inline(always)]
    fn from(n: u32) -> Self {
        Self::Num(n)
    }
}

impl From<String> for NumOrStr {
    #[inline(always)]
    fn from(s: String) -> Self {
        Self::Str(s)
    }
}

impl<'a> From<&'a str> for NumOrStr {
    #[inline(always)]
    fn from(s: &'a str) -> Self {
        Self::Str(s.into())
    }
}

#[derive(Serialize, Debug)]
#[serde(untagged)]
pub enum Value<'a> {
    Num(u32),
    Str(&'a str),
    Bool(bool),
}

////////////////////////////////////////////////////////////////////////////////
// ToDuration
////////////////////////////////////////////////////////////////////////////////

pub trait ToDuration: ToSecs + Copy + Sized {
    fn millis(self) -> Duration;
    fn nanos(self) -> Duration;
    fn micros(self) -> Duration;
}

impl ToDuration for u64 {
    #[inline(always)]
    fn millis(self) -> Duration {
        Duration::from_millis(self)
    }

    #[inline(always)]
    fn nanos(self) -> Duration {
        Duration::from_nanos(self)
    }

    #[inline(always)]
    fn micros(self) -> Duration {
        Duration::from_micros(self)
    }
}

////////////////////////////////////////////////////////////////////////////////
// ToSecs
////////////////////////////////////////////////////////////////////////////////

pub trait ToSecs: Copy + Sized {
    fn secs(self) -> Duration;

    #[cfg(feature = "duration_checked_float")]
    #[inline(always)]
    fn try_secs(self) -> Result<Duration, FromSecsError> {
        Ok(self.secs())
    }
}

impl ToSecs for u64 {
    #[inline(always)]
    fn secs(self) -> Duration {
        Duration::from_secs(self)
    }
}

impl ToSecs for f64 {
    #[inline(always)]
    fn secs(self) -> Duration {
        Duration::from_secs_f64(self)
    }

    #[cfg(feature = "duration_checked_float")]
    #[inline(always)]
    fn try_secs(self) -> Result<Duration, FromSecsError> {
        Duration::try_from_secs_f64(self)
    }
}

impl ToSecs for f32 {
    #[inline(always)]
    fn secs(self) -> Duration {
        Duration::from_secs_f32(self)
    }

    #[cfg(feature = "duration_checked_float")]
    #[inline(always)]
    fn try_secs(self) -> Result<Duration, FromSecsError> {
        Duration::try_from_secs_f32(self)
    }
}
