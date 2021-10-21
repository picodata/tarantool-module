use ffi::lua_State;
use std::{marker::PhantomData, convert::TryInto};

pub struct EmptyStack(pub *mut lua_State);

pub trait StatePtr {
    fn state_ptr(&self) -> *mut lua_State;
}

impl StatePtr for EmptyStack {
    fn state_ptr(&self) -> *mut lua_State {
        self.0
    }
}

pub struct Push<T, Stack> {
    parent: Stack,
    marker: PhantomData<T>,
}

impl<T, S> StatePtr for Push<T, S>
where
    S: StatePtr,
{
    fn state_ptr(&self) -> *mut lua_State {
        self.parent.state_ptr()
    }
}

pub trait PushInteger
where
    Self: StatePtr + Sized,
{
    fn push_integer(self, i: impl TryInto<isize>) -> Push<isize, Self> {
        unsafe {
            ffi::lua_pushinteger(self.state_ptr(), i.try_into().ok().unwrap())
        };
        Push { parent: self, marker: PhantomData }
    }
}

impl<T: StatePtr + Sized> PushInteger for T {}

pub trait ToInteger<I>
where
    Self: StatePtr + Sized,
    Self: HasIndex<I, isize>,
    I: Index,
{
    fn to_integer(&self, _: I) -> isize {
        unsafe { ffi::lua_tointeger(self.state_ptr(), I::INDEX) }
    }
}

impl<I, T> ToInteger<I> for T
where
    T: StatePtr + Sized,
    T: HasIndex<I, isize>,
    I: Index,
{}

pub trait HasIndex<I, T> {}

pub trait Index {
    const INDEX: i32;
}

macro_rules! impl_minus_index {
    ([ $head:tt $( $tail:tt )* ] $( $inner:tt )+ ) => {
        impl_minus_index!([ $( $tail )* ] Push<$head, $( $inner )+>)
    };
    ([  ] $( $s:tt )+) => {
        $( $s )+
    };
    ($trait:tt $tag:tt $idx:expr, $head:tt $( $tail:tt )*) => {
        pub trait $trait<$head> {
        }


        impl<Stack, $head, $( $tail ),*> $trait<$head>
            for impl_minus_index!([ $head $( $tail )* ] Stack)
        {
        }

        pub struct $tag;

        impl Index for $tag {
            const INDEX: i32 = $idx;
        }

        impl<Stack, T> HasIndex<$tag, T> for Stack
        where
            Self: $trait<T>,
        {
        }
    }
}

impl_minus_index!{HasMinus1 Minus1 -1, T}
impl_minus_index!{HasMinus2 Minus2 -2, T U}
impl_minus_index!{HasMinus3 Minus3 -3, T U V}
impl_minus_index!{HasMinus4 Minus4 -4, T U V W}

struct Table;

trait SetTable<I>
where
    Self: HasIndex<I, Table>
{

}

impl SetTable<Minus1> for Push<Table, EmptyStack> {}

trait Check<S>
where
    Self: HasMinus3<i32> + HasMinus1<S> + HasMinus2<bool> + HasMinus4<usize>,
    S: StringOrNumber,
{}

impl Check<String> for Push<String, Push<bool, Push<i32, Push<usize, ()>>>> {}

macro_rules! impl_type_set {
    ($name:tt : $( $t:tt )*) => {
        trait $name {}
        $( impl $name for $t {} )*
    }
}
impl_type_set!{ StringOrNumber: String i8 i16 i32 i64 u8 u16 u32 u64 }

