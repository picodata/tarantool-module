#[derive(PartialEq, Copy, Clone)]
pub enum ReflectionCode {
    Nchar       = 0,
    Nu8         = 1,
    Ni8         = 2,
    Nu16        = 3,
    Ni16        = 4,
    Nu32        = 5,
    Ni32        = 6,
    Nu64        = 7,
    Ni64        = 8,
    Nu128       = 9,
    Ni128       = 10,
    Nf32        = 11,
    Nf64        = 12,
    Nisize      = 13,
    Nusize      = 14,
    Nbool       = 15,
    NString     = 16,
    NStringLiteral = 17,
    //NReflection = 18,
    NUser       = 19,
    //NError      = 20,
}


#[macro_export]
macro_rules! make_collection {
    // map-like
    ($($k:expr => $v:expr),* $(,)?) =>
    {
        {
            use std::iter::{Iterator, IntoIterator};
            Iterator::collect(IntoIterator::into_iter([$(($k, $v),)*]))
        }
    };
    // set-like
    ($($v:expr),* $(,)?) =>
    {
        {
            use std::iter::{Iterator, IntoIterator};
            Iterator::collect(IntoIterator::into_iter([$($v,)*]))
        }
    };
}

#[inline(always)]
pub fn get_name_of_type<T>() -> &'static str {
    std::any::type_name::<T>()
}

#[macro_export]
macro_rules! refl_get_reflection_type_code_of {
    ($type:ty) => {
        {
            /*
            static ref TYPEHASHES: std::collections::HashMap<&str,ReflectionCode> = {make_collection!
            (
                &"u8"      => ReflectionCode::Nu8,
                &"i8"      => ReflectionCode::Ni8,
                &"i16"     => ReflectionCode::Ni16,
                &"u16"     => ReflectionCode::Nu16,
                &"i32"     => ReflectionCode::Ni32,
                &"u32"     => ReflectionCode::Nu32,
                &"f32"     => ReflectionCode::Nf32,
                &"f64"     => ReflectionCode::Nf64,
                &"bool"    => ReflectionCode::Nbool,
                &"String"  => ReflectionCode::NString,
            ) };*/
            use once_cell::sync::Lazy;
            use std::collections::HashMap;
            static TYPEHASHES: Lazy<HashMap<String,ReflectionCode> > = Lazy::new( ||
            {
                make_collection!
                (
                    "u8".to_string()      => ReflectionCode::Nu8,
                    "i8".to_string()      => ReflectionCode::Ni8,
                    "i16".to_string()     => ReflectionCode::Ni16,
                    "u16".to_string()     => ReflectionCode::Nu16,
                    "i32".to_string()     => ReflectionCode::Ni32,
                    "u32".to_string()     => ReflectionCode::Nu32,
                    "f32".to_string()     => ReflectionCode::Nf32,
                    "f64".to_string()     => ReflectionCode::Nf64,
                    "bool".to_string()    => ReflectionCode::Nbool,
                    "String".to_string()  => ReflectionCode::NString,
                )
            } );
            let strname = get_name_of_type::<$type>();
            match TYPEHASHES.get( &strname.to_string() ) {
                Some(entry) => entry.clone(),
                None => ReflectionCode::NUser,
            }
        }
    }
}


pub trait GetTypeCodeTrait {
    fn get_type_code() -> ReflectionCode;
    fn get_type_code_from( &self ) -> ReflectionCode
    where Self : std::default::Default {
        Self::get_type_code()
    }
}

impl GetTypeCodeTrait for char {
    #[inline(always)]
    fn get_type_code() -> ReflectionCode  {
        ReflectionCode::Nchar
    }
}

impl GetTypeCodeTrait for u8 {
    #[inline(always)]
    fn get_type_code() -> ReflectionCode  {
        ReflectionCode::Nu8
    }
}

impl GetTypeCodeTrait for i8 {
    #[inline(always)]
    fn get_type_code() -> ReflectionCode  {
        ReflectionCode::Ni8
    }
}

impl GetTypeCodeTrait for u16 {
    #[inline(always)]
    fn get_type_code() -> ReflectionCode  {
        ReflectionCode::Nu16
    }
}

impl GetTypeCodeTrait for i16 {
    #[inline(always)]
    fn get_type_code() -> ReflectionCode  {
        ReflectionCode::Ni16
    }
}

impl GetTypeCodeTrait for u32 {
    #[inline(always)]
    fn get_type_code() -> ReflectionCode  {
        ReflectionCode::Nu32
    }
}

impl GetTypeCodeTrait for i32 {
    #[inline(always)]
    fn get_type_code() -> ReflectionCode  {
        ReflectionCode::Ni32
    }
}

impl GetTypeCodeTrait for u64 {
    #[inline(always)]
    fn get_type_code() -> ReflectionCode  {
        ReflectionCode::Nu64
    }
}

impl GetTypeCodeTrait for i64 {
    #[inline(always)]
    fn get_type_code() -> ReflectionCode  {
        ReflectionCode::Ni64
    }
}

impl GetTypeCodeTrait for u128 {
    #[inline(always)]
    fn get_type_code() -> ReflectionCode  {
        ReflectionCode::Nu128
    }
}

impl GetTypeCodeTrait for i128 {
    #[inline(always)]
    fn get_type_code() -> ReflectionCode  {
        ReflectionCode::Ni128
    }
}

impl GetTypeCodeTrait for f32 {
    #[inline(always)]
    fn get_type_code() -> ReflectionCode  {
        ReflectionCode::Nf32
    }
}

impl GetTypeCodeTrait for f64 {
    #[inline(always)]
    fn get_type_code() -> ReflectionCode  {
        ReflectionCode::Nf64
    }
}

impl GetTypeCodeTrait for usize {
    #[inline(always)]
    fn get_type_code() -> ReflectionCode  {
        ReflectionCode::Nusize
    }
}

impl GetTypeCodeTrait for isize {
    #[inline(always)]
    fn get_type_code() -> ReflectionCode  {
        ReflectionCode::Nisize
    }
}

impl GetTypeCodeTrait for String {
    #[inline(always)]
    fn get_type_code() -> ReflectionCode  {
        ReflectionCode::NString
    }
}

impl GetTypeCodeTrait for &'static str {
    #[inline(always)]
    fn get_type_code() -> ReflectionCode  {
        ReflectionCode::NStringLiteral
    }
}
