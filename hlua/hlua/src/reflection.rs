extern crate lazy_static;
use lazy_static::lazy_static;
use std::any::Any;

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
    NReflection = 18,
    NUser       = 19,
    NError      = 20,
}

#[macro_export]
macro_rules! refl_internal_hash_by_typeid {
    ($typeid_var:expr) => {
        {
            use std::collections::hash_map::DefaultHasher;
            let mut hasher : DefaultHasher = DefaultHasher::new();
            <std::any::TypeId as std::hash::Hash>::hash(& $typeid_var, & mut hasher);
            <DefaultHasher as std::hash::Hasher>::finish(&hasher)
        }
    }
}
//typeid() и TypeId::of
// например,TypeId::of::<String>() == s.type_id()
struct StaticInit<'a, Type > {
    data: &'a Type,
}

#[macro_export]
macro_rules! refl_internal_hash_of {
    ($type:ty) =>
    {
        {
            lazy_static! {
                static ref type_var : &'static $type = {
                    &<$type as std::default::Default>::default()
                };
            }
            //const type_var : StaticInit<'static, $type> = StaticInit{ data : &<$type as std::default::Default>::default() };
            //static type_var: $type ;//= unsafe { std::mem::MaybeUninit::zeroed().assume_init() };//std::default::Default::default();
            let typeid_var : std::any::TypeId = <$type as std::any::Any>::type_id(type_var);
            refl_internal_hash_by_typeid!( typeid_var )
        }
    }
}

#[macro_export]
macro_rules! refl_internal_typehash {
    ($typehash:expr) => {
        {
            let typeid_var : std::any::TypeId = std::any::Any::type_id($typehash);
            refl_internal_hash_by_typeid!( typeid_var )
        }
    };
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

pub fn refl_get_internal_types_hashes() -> &'static std::collections::HashMap<u64,
                                                               ReflectionCode> {
    static TYPEHASHES: std::collections::HashMap<u64,ReflectionCode> = make_collection!
    (
        refl_internal_hash_of!(u8)      => ReflectionCode::Nu8,
        refl_internal_hash_of!(i8)      => ReflectionCode::Ni8,
        refl_internal_hash_of!(i16)     => ReflectionCode::Ni16,
        refl_internal_hash_of!(u16)     => ReflectionCode::Nu16,
        refl_internal_hash_of!(i32)     => ReflectionCode::Ni32,
        refl_internal_hash_of!(u32)     => ReflectionCode::Nu32,
        refl_internal_hash_of!(f32)     => ReflectionCode::Nf32,
        refl_internal_hash_of!(f64)     => ReflectionCode::Nf64,
        refl_internal_hash_of!(bool)    => ReflectionCode::Nbool,
        refl_internal_hash_of!(String)  => ReflectionCode::NString,
    );
    &TYPEHASHES
}

#[macro_export]
macro_rules! refl_get_reflection_type_code_by_typeid {
    ($typeid:expr) => {
        {
            let current_type_hash = refl_internal_hash_by_typeid!($typeid);
            let TYPEHASHES : &'static std::collections::
                HashMap<u64, ReflectionCode> = refl_get_internal_types_hashes();
            if TYPEHASHES.contains_key( &current_type_hash ) {
                TYPEHASHES[ &current_type_hash ]
            } else {
                ReflectionCode::NUser
            }
        }
    }
}

#[macro_export]
macro_rules! refl_get_reflection_type_code_of {
    ($type:ty) => {
        {
            let current_type_hash = refl_internal_hash_of!($type);
            refl_get_reflection_type_code_by_typeid!( current_type_hash )
        }
    }
}


pub trait GetTypeCodeTrait {
    fn get_type_code() -> ReflectionCode;
    fn get_type_code_from( &self ) -> ReflectionCode
    where Self : std::default::Default {
        //assert_eq!( refl_get_reflection_typecode_by_typeid!( self.type_id() ), ReflectionCode::Ni8 );
        Self::get_type_code()
        //refl_get_reflection_type_code!( self.type_id() )
        //let typeid = std::any::TypeId::of::<Self>();
        //let current_type_hash = refl_internal_hash_by_typeid!(typeid);
        //let typeid : std::any::TypeId = std::any::TypeId::of::<Self>();
        //let typeid : std::any::TypeId = std::any::Any::type_id(&self);
        /*
        let typeid : std::any::TypeId = std::any::Any::type_id(&self);
        {
            let current_type_hash = refl_internal_hash_by_typeid!(typeid);
            let TYPEHASHES : &'static std::collections::
                HashMap<u64, ReflectionCode> = refl_get_internal_types_hashes();
            if TYPEHASHES.contains_key( &current_type_hash ) {
                TYPEHASHES[ &current_type_hash ]
            } else {
                ReflectionCode::NUser
            }
        }
        */
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


pub enum ReflectionData {
    Tchar(char),
    Tu8(u8),
    Ti8(i8),
    Tu16(u16),
    Ti16(i16),
    Tu32(u32),
    Ti32(i32),
    Tu64(u64),
    Ti64(i64),
    Tu128(u128),
    Ti128(i128),
    Tf32(f32),
    Tf64(f64),
    Tisize(isize),
    Tusize(usize),
    TString(String),
    TStringLiteral(&'static str),
    //TReflection( Box<dyn BaseReflection> ),
    TUser( Box<dyn Any> ),
    Error(),
}

impl ReflectionData {
    fn get_type_code( &self ) -> ReflectionCode {
        use ReflectionData::*;
        match self {
            Tchar(_y) => ReflectionCode::Nchar,
            Tu8(_y) => ReflectionCode::Nu8,
            Ti8(_y) => ReflectionCode::Ni8,
            Tu16(_y) => ReflectionCode::Nu16,
            Ti16(_y) => ReflectionCode::Ni16,
            Tu32(_y) => ReflectionCode::Nu32,
            Ti32(_y) => ReflectionCode::Ni32,
            Tu64(_y) => ReflectionCode::Nu64,
            Ti64(_y) => ReflectionCode::Ni64,
            Tu128(_y) => ReflectionCode::Nu128,
            Ti128(_y) => ReflectionCode::Ni128,
            Tf32(_y) => ReflectionCode::Nf32,
            Tf64(_y) => ReflectionCode::Nf64,
            Tisize(_y) => ReflectionCode::Nisize,
            Tusize(_y) => ReflectionCode::Nusize,
            TString(_y) => ReflectionCode::NString,
            TStringLiteral(_) => ReflectionCode::NStringLiteral,
            //TReflection( _y ) => ReflectionCode::NReflection,
            TUser( _y ) => ReflectionCode::NUser,
            Error() => ReflectionCode::NError,
            _ => ReflectionCode::NError,
        }
    }
}