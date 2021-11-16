use crate::{AsMutLua};

#[macro_export]
macro_rules! implement_lua_push {
    ($ty:ty, $cb:expr) => {
        impl<'lua, L> $crate::Push<L> for $ty where L: $crate::AsMutLua<'lua> {
            type Err = $crate::Void;      // TODO: use ! instead
            #[inline]
            fn push_to_lua(self, lua: L) -> Result<$crate::PushGuard<L>, ($crate::Void, L)> {
                Ok($crate::push_userdata(self, lua, $cb))
            }
        }
        impl<'lua, L> $crate::PushOne<L> for $ty where L: $crate::AsMutLua<'lua> {
        }
    };
}

#[macro_export]
macro_rules! implement_lua_read {
    ($ty:ty) => {
        impl<'s, 'c> hlua::LuaRead<&'c mut hlua::InsideCallback> for &'s mut $ty {
            #[inline]
            fn lua_read_at_position(lua: &'c mut hlua::InsideCallback, index: i32) -> Result<&'s mut $ty, &'c mut hlua::InsideCallback> {
                // FIXME:
                unsafe { ::std::mem::transmute($crate::read_userdata::<$ty>(lua, index)) }
            }
        }

        impl<'s, 'c> hlua::LuaRead<&'c mut hlua::InsideCallback> for &'s $ty {
            #[inline]
            fn lua_read_at_position(lua: &'c mut hlua::InsideCallback, index: i32) -> Result<&'s $ty, &'c mut hlua::InsideCallback> {
                // FIXME:
                unsafe { ::std::mem::transmute($crate::read_userdata::<$ty>(lua, index)) }
            }
        }

        impl<'s, 'b, 'c> hlua::LuaRead<&'b mut &'c mut hlua::InsideCallback> for &'s mut $ty {
            #[inline]
            fn lua_read_at_position(lua: &'b mut &'c mut hlua::InsideCallback, index: i32) -> Result<&'s mut $ty, &'b mut &'c mut hlua::InsideCallback> {
                let ptr_lua = lua as *mut &mut hlua::InsideCallback;
                let deref_lua = unsafe { ::std::ptr::read(ptr_lua) };
                let res = Self::lua_read_at_position(deref_lua, index);
                match res {
                    Ok(x) => Ok(x),
                    _ => Err(lua)
                }
            }
        }

        impl<'s, 'b, 'c> hlua::LuaRead<&'b mut &'c mut hlua::InsideCallback> for &'s $ty {
            #[inline]
            fn lua_read_at_position(lua: &'b mut &'c mut hlua::InsideCallback, index: i32) -> Result<&'s $ty, &'b mut &'c mut hlua::InsideCallback> {
                let ptr_lua = lua as *mut &mut hlua::InsideCallback;
                let deref_lua = unsafe { ::std::ptr::read(ptr_lua) };
                let res = Self::lua_read_at_position(deref_lua, index);
                match res {
                    Ok(x) => Ok(x),
                    _ => Err(lua)
                }
            }
        }
    };
}

#[macro_export]
macro_rules! c_ptr {
    ($s:literal) => {
        ::std::concat!($s, "\0").as_bytes().as_ptr() as *mut i8
    };
}

#[macro_export]
macro_rules! lua_error {
    ($l:expr, $msg:literal) => {
        {
            $crate::luaL_error($l, c_ptr!($msg));
            unreachable!()
        }
    }
}


#[inline(always)]
pub unsafe fn dereference_and_corrupt_mut_ref< 'a, R>( refr : & mut R) -> R
where
    R : 'a
{
    let mut ret : R = std::mem::MaybeUninit::uninit().assume_init();
    std::mem::swap( refr, & mut ret );
    std::mem::forget( refr );
    ret
}

#[inline(always)]
pub fn start_read_table<'lua, L>( lua : & mut L, index : &i32) -> bool
where L: AsMutLua<'lua> {
    unsafe {
        ffi::lua_istable(
            lua.as_mut_lua().state_ptr(),
            index.clone() )
    }
}


#[macro_export]
macro_rules! lua_push {
    ($lua:expr, $value:expr,$error_reaction:expr ) => {
        unsafe {
            match ($value).push_to_lua( dereference_and_corrupt_mut_ref( $lua ) ) {
                Ok(mut guard) => {
                    std::mem::swap($lua , & mut guard.lua );
                    guard.forget();
                    true
                }
                Err( ( _lua_push_error, _ ) ) => {
                    $error_reaction;
                    false
                }
            }
        }
    }
}


#[macro_export]
macro_rules! lua_get {
    ($lua:expr,
     $number_of_retvalues:expr,
     $success_reaction:expr,
     $error_reaction:expr,
     $expected_type:ty ) => {
        {
            let new_lua = unsafe { hlua::PushGuard::new( $lua, 0 ) };
            match LuaRead::lua_read_at_position( new_lua, -$number_of_retvalues ) {
                Ok(ret_value) => {
                    $success_reaction;
                    Some(ret_value)
                },
                Err( _read_err ) => {
                    $error_reaction;
                    None
                },
            }
        }
    }
}

