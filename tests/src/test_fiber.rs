use std::{
    cell::RefCell,
    rc::Rc,
    time::Duration,
    any::TypeId,
    ffi::CStr,
};

use tarantool::{
    fiber::{self, fiber_yield, is_cancelled, sleep, Cond, Fiber, FiberAttr},
    ffi::c_void,
};

pub fn test_fiber_new() {
    let mut fiber = Fiber::new("test_fiber", &mut |_| 0);
    fiber.set_joinable(true);
    fiber.start(());
    fiber.join();
}

pub fn test_fiber_new_with_attr() {
    let mut attr = FiberAttr::new();
    attr.set_stack_size(100_000).unwrap();

    let mut fiber = Fiber::new_with_attr("test_fiber", &attr, &mut |_| 0);
    fiber.set_joinable(true);
    fiber.start(());
    fiber.join();
}

pub fn test_fiber_arg() {
    let mut fiber = Fiber::new("test_fiber", &mut |x| {
        assert_eq!(*x, 99);
        0
    });
    fiber.set_joinable(true);
    fiber.start(99);
    fiber.join();
}

pub fn test_fiber_cancel() {
    let mut fiber = Fiber::new("test_fiber", &mut |_| {
        assert_eq!(is_cancelled(), false);
        sleep(0.01);
        assert_eq!(is_cancelled(), true);
        0
    });
    fiber.set_joinable(true);
    fiber.start(());
    fiber.cancel();
    fiber.join();
}

pub fn test_fiber_wake() {
    let mut fiber = Fiber::new("test_fiber", &mut |_| {
        fiber_yield();
        0
    });
    fiber.set_joinable(true);
    fiber.start(());
    sleep(0.01);
    fiber.wakeup();
    fiber.join();
}

pub fn test_fiber_wake_multiple() {
    let res = Rc::new(RefCell::new(vec![]));
    let mut fibers = vec![];
    for (i, c) in (1..).zip(&['a', 'b', 'c']) {
        let mut fiber = Fiber::new(
            &format!("test_fiber_{}", c),
            &mut |r: Box<Rc<RefCell<Vec<i32>>>>| {
                fiber_yield();
                r.borrow_mut().push(i);
                0
            }
        );
        fiber.start(res.clone());
        fiber.wakeup();
        fibers.push(fiber);
    }

    for f in &mut fibers {
        f.set_joinable(true);
    }

    res.borrow_mut().push(0);
    for f in fibers {
        f.join();
    }
    res.borrow_mut().push(4);

    let res = res.borrow().iter().copied().collect::<Vec<_>>();
    // This is what we want:
    // assert_eq(res, vec![0, 1, 2, 3, 4]);
    // This is what we get:
    assert_eq!(res, vec![0, 3, 3, 3, 4]);
    // Because `Fiber` doesn't work with closures. `i` is passed by reference
    // and by the time the first fiber starts executing, it is equal to 3.
    // This is actually undefined behavior, so adding this test is probably a
    // bad idea
}

pub fn test_fiber_cond_signal() {
    let cond = Rc::new(Cond::new());
    let mut fiber = Fiber::new("test_fiber", &mut |cond: Box<Rc<Cond>>| {
        (*cond).wait();
        0
    });
    fiber.set_joinable(true);
    fiber.start(cond.clone());
    sleep(0.01);
    cond.signal();
    fiber.join();
}

pub fn test_fiber_cond_broadcast() {
    let cond = Rc::new(Cond::new());

    let mut fiber_a = Fiber::new("test_fiber_a", &mut |cond: Box<Rc<Cond>>| {
        (*cond).wait();
        0
    });
    fiber_a.set_joinable(true);
    fiber_a.start(cond.clone());

    let mut fiber_b = Fiber::new("test_fiber_b", &mut |cond: Box<Rc<Cond>>| {
        (*cond).wait();
        0
    });
    fiber_b.set_joinable(true);
    fiber_b.start(cond.clone());

    sleep(0.01);
    cond.broadcast();
    fiber_a.join();
    fiber_b.join();
}

pub fn test_fiber_cond_timeout() {
    let cond = Rc::new(Cond::new());
    let mut fiber = Fiber::new("test_fiber", &mut |cond: Box<Rc<Cond>>| {
        let r = (*cond).wait_timeout(Duration::from_secs_f32(0.01));
        assert!(!r);
        0
    });
    fiber.set_joinable(true);
    fiber.start(cond.clone());
    sleep(0.02);
    cond.signal();
    fiber.join();
}

////////////////////////////////////////////////////////////////////////////////
// New
////////////////////////////////////////////////////////////////////////////////

pub fn test_immediate() {
    let jh = fiber::Builder::new()
        .func(|| 69)
        .start()
        .unwrap();
    let res = jh.join();
    assert_eq!(res, 69);

    let jh = fiber::start(|| 420);
    let res = jh.join();
    assert_eq!(res, 420);
}

pub fn test_immediate_with_attrs() {
    let jh = fiber::Builder::new()
        .name("boo")
        .stack_size(100_000).unwrap()
        .func(|| 42)
        .start()
        .unwrap();
    let res = jh.join();
    assert_eq!(res, 42);
}

pub fn test_multiple_immediate() {
    let mut res = vec![];
    let fibers = vec![vec![1, 2], vec![3, 4], vec![5, 6]]
        .into_iter()
        .map(|v|
            fiber::start(move || {
                v.into_iter().map(|e| e + 1).collect::<Vec::<_>>()
            })
        )
        .collect::<Vec<_>>();
    res.push(1);
    res.extend(
        fibers.into_iter()
            .map(fiber::JoinHandle::join)
            .flatten()
    );
    res.push(8);
    assert_eq!(res, vec![1, 2, 3, 4, 5, 6, 7, 8]);
}

pub fn test_unit_immediate() {
    let jh = fiber::Builder::new()
        .func(|| ())
        .start()
        .unwrap();
    let () = jh.join();

    let () = fiber::start_proc(|| ()).join();
}

pub fn test_unit_immediate_with_attrs() {
    let jh = fiber::Builder::new()
        .name("boo")
        .stack_size(100_000).unwrap()
        .proc(|| ())
        .start()
        .unwrap();
    let () = jh.join();
}

pub fn test_multiple_unit_immediate() {
    let res = Rc::new(RefCell::new(vec![]));
    let fibers = vec![vec![1, 2], vec![3, 4], vec![5, 6]]
        .into_iter()
        .map(|v| {
            let res_ref = res.clone();
            fiber::start_proc(move || {
                res_ref.borrow_mut().extend(
                    v.into_iter().map(|e| e + 1).collect::<Vec::<_>>()
                )
            })
        })
        .collect::<Vec<_>>();
    res.borrow_mut().push(8);
    for f in fibers {
        f.join()
    }
    res.borrow_mut().push(9);
    let res = res.borrow().iter().copied().collect::<Vec<_>>();
    assert_eq!(res, vec![2, 3, 4, 5, 6, 7, 8, 9]);
}

pub fn test_deferred() {
    let jh = fiber::Builder::new()
        .func(|| 13)
        .defer()
        .unwrap();
    assert_eq!(jh.join(), 13);

    let jh = fiber::defer(|| 42);
    assert_eq!(jh.join(), 42);
}

pub fn test_deferred_with_attrs() {
    let res = fiber::Builder::new()
        .name("boo")
        .stack_size(100_000).unwrap()
        .func(|| 15)
        .defer()
        .unwrap()
        .join();
    assert_eq!(res, 15);
}

pub fn test_multiple_deferred() {
    let mut res = vec![];
    let fibers = vec![vec![1, 2], vec![3, 4], vec![5, 6]]
        .into_iter()
        .map(|v|
            fiber::defer(move || {
                v.into_iter().map(|e| e + 1).collect::<Vec::<_>>()
            })
        )
        .collect::<Vec<_>>();
    res.push(1);
    res.extend(
        fibers.into_iter()
            .map(fiber::JoinHandle::join)
            .flatten()
    );
    res.push(8);
    assert_eq!(res, vec![1, 2, 3, 4, 5, 6, 7, 8]);
}

pub fn test_unit_deferred() {
    let jh = fiber::Builder::new()
        .proc(|| ())
        .defer()
        .unwrap();
    let () = jh.join();

    let res = std::cell::Cell::new(0);
    let jh = fiber::defer_proc(|| res.set(42));
    assert_eq!(res.get(), 0);
    jh.join();
    assert_eq!(res.get(), 42);
}

pub fn test_unit_deferred_with_attrs() {
    let () = fiber::Builder::new()
        .name("boo")
        .stack_size(100_000).unwrap()
        .proc(|| ())
        .defer()
        .unwrap()
        .join();
}

pub fn test_multiple_unit_deferred() {
    let res = Rc::new(RefCell::new(vec![]));
    let fibers = vec![vec![1, 2], vec![3, 4], vec![5, 6]]
        .into_iter()
        .map(|v| {
            let res_ref = res.clone();
            fiber::defer_proc(move ||
                res_ref.borrow_mut().extend(
                    v.into_iter().map(|e| e + 1).collect::<Vec::<_>>()
                )
            )
        })
        .collect::<Vec<_>>();
    res.borrow_mut().push(1);
    for f in fibers {
        f.join()
    }
    res.borrow_mut().push(8);
    let res = res.borrow().iter().copied().collect::<Vec<_>>();
    assert_eq!(res, vec![1, 2, 3, 4, 5, 6, 7, 8]);
}

use tarantool::{ffi::lua as ffi, hlua::*};

macro_rules! c_ptr {
    ($s:literal) => {
        ::std::concat![$s, "\0"].as_bytes().as_ptr() as *mut i8
    }
}

////////////////////////////////////////////////////////////////////////////////
/// FuncOnce
////////////////////////////////////////////////////////////////////////////////

// f: FnOnce() -> T
// rc_f = Rc::new(f)                    : rc_f -*> RcBox { 1, 1, f }
// ud = ffi::lua_newuserdata(lua, size) : ud -> LuaUserData {}
//                                      : lua@{top} -> LuaUserData {}
//
// call_wrapper({ f, dropped? }) : f.call(); drop(f); true -> dropped?
//   gc_wrapper({ f, dropped? }) : if !dropped? { drop(f) }
//
struct FuncOnce<F>(F);

impl<'lua, L, F, T> Push<L> for FuncOnce<F>
where
    L: AsMutLua<'lua>,
    F: FnOnce() -> T,
    T: for<'a> Push<&'a mut InsideCallback>,
    T: 'static,
{
    type Err = Void;

    fn push_to_lua(self, mut lua: L) -> Result<PushGuard<L>, (Void, L)> {
        unsafe {
            let lptr = lua.as_mut_lua().state_ptr();
            // pushing the function pointer as a userdata
            let FuncOnce(f) = self;
            push_userdata(lptr, JustUserData::new(f));

            // pushing wrapper as a closure
            ffi::lua_pushcclosure(lptr, wrap_call_once::<F, T>, 1);
            Ok(PushGuard::new(lua, 1))
        }
    }
}

impl<'lua, F, T, L> PushOne<L> for FuncOnce<F>
where
    L: AsMutLua<'lua>,
    F: FnOnce() -> T,
    T: for<'a> Push<&'a mut InsideCallback>,
    T: 'static,
{}

unsafe fn push_func_once<F, T>(lua: *mut ffi::lua_State, f: F)
where
    F: FnOnce() -> T,
    T: 'static,
{
    push_userdata(lua, JustUserData::new(f));
    ffi::lua_pushcclosure(lua, wrap_call_once::<F, T>, 1);
}

unsafe extern "C" fn wrap_call_once<F, T>(lua: *mut ffi::lua_State) -> i32
where
    F: FnOnce() -> T,
    T: 'static,
{
    // loading the object that we want to call from the Lua context
    let ud_ptr = ffi::lua_touserdata(lua, ffi::lua_upvalueindex(1));
    let maybe_ud = (ud_ptr as *mut JustUserData<F>).as_mut();

    let ud = maybe_ud.unwrap_or_else(|| {
        // lua_touserdata returned NULL
        ffi::luaL_error(lua, c_ptr!("failed to extract upvalue"));
        unreachable!();
    });

    // put None back into userdata
    let f = ud.take().unwrap_or_else(|| {
        // userdata contains None
        ffi::luaL_error(
            lua, c_ptr!("rust FnOnce callback was called more than once")
        );
        unreachable!();
    });

    // call f and drop it afterwards
    let res = f();

    // return results to lua
    push_userdata(lua, TypedUserData::new(res));
    1
}

////////////////////////////////////////////////////////////////////////////////
/// TypedUserData
////////////////////////////////////////////////////////////////////////////////

// We need `repr(C)` here so that we can use pointers as both pointer to
// `UserDataUserDataWithTypeId` and pointer to `TypeId`
#[repr(C)]
struct TypedUserData<T>
where
    T: 'static + Sized,
{
    type_id: TypeId,
    value: Option<T>,
}

// TODO: add pointer alignment checks
impl<T> TypedUserData<T> {
    fn new(value: T) -> Self {
        Self { type_id: TypeId::of::<T>(), value: Some(value) }
    }

    fn is_good(ptr: *const c_void) -> bool
    where
        T: 'static,
    {
        unsafe {
            (ptr as *mut TypeId).as_ref()
        }
            .map(|&tid| tid == TypeId::of::<T>())
            .unwrap_or(false)
    }

    fn try_from_ptr_mut<'lua>(ptr: *mut c_void) -> Option<&'lua mut Self> {
        if !Self::is_good(ptr) {
            return None;
        }
        unsafe { (ptr as *mut Self).as_mut() }
    }
}

impl<T: 'static> UserDataContainer for TypedUserData<T> {
    type Contained = T;

    fn new(value: Self::Contained) -> Self {
        Self {
            type_id: TypeId::of::<Self::Contained>(),
            value: Some(value),
        }
    }

    fn take(&mut self) -> Option<Self::Contained> {
        self.value.take()
    }
}

impl<'lua, 'a, T, L> LuaRead<L> for &'a mut TypedUserData<T>
where
    T: 'static,
    L: AsMutLua<'lua>,
{
    fn lua_read_at_position(lua: L, index: i32) -> Result<Self, L> {
        let ud_ptr = unsafe {
            ffi::lua_touserdata(lua.as_lua().state_ptr(), index)
        };

        if let Some(ud) = TypedUserData::<T>::try_from_ptr_mut(ud_ptr) {
            Ok(ud)
        } else {
            Err(lua)
        }
    }
}

impl<'lua, T, L> Push<L> for TypedUserData<T>
where
    L: AsMutLua<'lua>,
{
    type Err = Void;

    fn push_to_lua(self, mut lua: L) -> Result<PushGuard<L>, (Self::Err, L)> {
        unsafe {
            push_userdata(lua.as_mut_lua().state_ptr(), self);
            Ok(PushGuard::new(lua, 1))
        }
    }
}

impl<'lua, T, L> PushOne<L> for TypedUserData<T>
where
    L: AsMutLua<'lua>
{}

////////////////////////////////////////////////////////////////////////////////
/// JustUserData
////////////////////////////////////////////////////////////////////////////////

struct JustUserData<T>(Option<T>)
where
    T: Sized;

impl<T> UserDataContainer for JustUserData<T> {
    type Contained = T;

    fn new(value: Self::Contained) -> Self {
        Self(Some(value))
    }

    fn take(&mut self) -> Option<Self::Contained> {
        self.0.take()
    }
}

impl<'lua, T, L> Push<L> for JustUserData<T>
where
    L: AsMutLua<'lua>,
{
    type Err = Void;

    fn push_to_lua(self, mut lua: L) -> Result<PushGuard<L>, (Self::Err, L)> {
        unsafe {
            push_userdata(lua.as_mut_lua().state_ptr(), self);
            Ok(PushGuard::new(lua, 1))
        }
    }
}

impl<'lua, T, L> PushOne<L> for JustUserData<T>
where
    L: AsMutLua<'lua>
{}

////////////////////////////////////////////////////////////////////////////////
/// UserDataContainer
////////////////////////////////////////////////////////////////////////////////

trait UserDataContainer {
    type Contained;
    const SIZE_OF: usize = std::mem::size_of::<Self::Contained>();

    fn new(value: Self::Contained) -> Self;
    fn take(&mut self) -> Option<Self::Contained>;
}

unsafe fn push_userdata<U>(lua: *mut ffi::lua_State, ud: U)
where
    U: UserDataContainer,
{
    let ud_ptr = ffi::lua_newuserdata(lua, U::SIZE_OF);
    std::ptr::write(ud_ptr as *mut U, ud);

    if std::mem::needs_drop::<U::Contained>() {
        // Creating a metatable.
        ffi::lua_newtable(lua);

        // Index "__gc" in the metatable calls the object's destructor.
        ffi::lua_pushstring(lua, c_ptr!("__gc"));
        ffi::lua_pushcfunction(lua, wrap_gc::<U>);
        ffi::lua_settable(lua, -3);

        ffi::lua_setmetatable(lua, -2);
    }

    /// A callback for the "__gc" event. It checks if the value was moved out
    /// and if not it drops the value.
    unsafe extern "C" fn wrap_gc<U>(lua: *mut ffi::lua_State) -> i32
    where
        U: UserDataContainer,
    {
        let ud_ptr = ffi::lua_touserdata(lua, 1);
        let ud = (ud_ptr as *mut U)
            .as_mut()
            .expect("__gc called with userdata pointing to NULL");
        drop(ud.take());

        0
    }
}

////////////////////////////////////////////////////////////////////////////////
///
////////////////////////////////////////////////////////////////////////////////

pub fn func_once_gc() {
    static mut DROPPED_TIMES: isize = 0;
    static mut CALLED_TIMES: isize = 0;

    struct Foo;
    impl Drop for Foo {
        fn drop(&mut self) {
            unsafe { DROPPED_TIMES += 1; }
        }
    }
    let foo = Foo;
    {
        Lua::new()
            .set("drop_foo", FuncOnce(move || {
                unsafe { CALLED_TIMES += 1 };
                drop(foo)
            }));
    }
    assert_eq!(unsafe { DROPPED_TIMES }, 1);
    assert_eq!(unsafe { CALLED_TIMES }, 0);
}

pub fn func_once_call() {
    static mut DROPPED_TIMES: isize = 0;
    static mut CALLED_TIMES: isize = 0;

    struct Foo;
    impl Drop for Foo {
        fn drop(&mut self) {
            unsafe { DROPPED_TIMES += 1; }
        }
    }
    let foo = Foo;
    let res: i32 = {
        let mut lua = Lua::new();
        lua.set("drop_foo", FuncOnce(move || {
            unsafe { CALLED_TIMES += 1 };
            drop(foo);
            13
        }));
        lua.execute("return drop_foo()").unwrap()
    };
    assert_eq!(unsafe { DROPPED_TIMES }, 1);
    assert_eq!(unsafe { CALLED_TIMES }, 1);
    assert_eq!(res, 13);
}

pub fn func_once_call_twice() {
    static mut DROPPED_TIMES: isize = 0;
    static mut CALLED_TIMES: isize = 0;

    struct Foo;
    impl Drop for Foo {
        fn drop(&mut self) {
            unsafe { DROPPED_TIMES += 1; }
        }
    }

    let foo = Foo;
    let msg = {
        let mut lua = Lua::new();
        lua.set("drop_foo", FuncOnce(move || {
            unsafe { CALLED_TIMES += 1 };
            drop(foo)
        }));
        lua.execute::<()>("drop_foo()").unwrap();
        match lua.execute::<()>("drop_foo()").unwrap_err() {
            LuaError::ExecutionError(msg) => msg,
            _ => panic!("unexpected error kind"),
        }
    };
    assert_eq!(unsafe { DROPPED_TIMES }, 1);
    assert_eq!(unsafe { CALLED_TIMES }, 1);
    assert_eq!(&msg, "[string \"chunk\"]:1: rust FnOnce callback was called more than once");
}

pub fn test_multiple_deferred_NOTcorrect() {
    let mut res = vec![];
    let mut references = vec![];
    let mut lua = crate::hlua::global();
    let lptr = lua.as_lua().state_ptr();
    for v in vec![vec![1, 2], vec![3, 4], vec![5, 6]] {
        unsafe {
            ffi::lua_getglobal(lptr, c_ptr!("require"));
            ffi::lua_pushstring(lptr, c_ptr!("fiber"));
            if ffi::lua_pcall(lptr, 1, 1, 0) == ffi::LUA_ERRRUN {
                panic!(
                    "lua_pcall failed: {:?}",
                    CStr::from_ptr(ffi::lua_tostring(lptr, -1)),
                )
            };
            ffi::lua_getfield(lptr, -1, c_ptr!("new"));
            push_func_once(lptr,
                move || v.into_iter().map(|e| e + 1).collect::<Vec<_>>()
            );
            if ffi::lua_pcall(lptr, 1, 1, 0) == ffi::LUA_ERRRUN {
                panic!(
                    "lua_pcall failed: {:?}",
                    CStr::from_ptr(ffi::lua_tostring(lptr, -1)),
                )
            };
            ffi::lua_getfield(lptr, -1, c_ptr!("set_joinable"));
            ffi::lua_pushvalue(lptr, -2);
            ffi::lua_pushboolean(lptr, true as i32);
            if ffi::lua_pcall(lptr, 2, 0, 0) == ffi::LUA_ERRRUN {
                panic!(
                    "lua_pcall failed: {:?}",
                    CStr::from_ptr(ffi::lua_tostring(lptr, -1)),
                )
            };
            let fiber_ref = ffi::luaL_ref(lptr, ffi::LUA_REGISTRYINDEX);
            references.push(dbg!(fiber_ref));
        }
        // let mut require: LuaFunction<_> = lua.get("require").unwrap();
        // let mut fiber_module: LuaTable<_> = require.call_with_args("fiber").unwrap();
        // let mut fiber_new: LuaFunction<_> = fiber_module.get("new").unwrap();
        // unsafe { dbg!((ffi::lua_gettop(lptr), std::ffi::CStr::from_ptr(ffi::lua_typename(lptr, ffi::lua_type(lptr, -1))))); }
        // let fiber_func = FuncOnce(
        //     move || v.into_iter().map(|e| e + 1).collect::<Vec<_>>()
        // );
        // let mut fiber: LuaTable<_> = fiber_new.call_with_args(fiber_func).unwrap();
        // let () = fiber.method("set_joinable", true).unwrap();
        // references.push(fiber.into_ref().unwrap());

        // unsafe {
        //     let lptr = lua.as_mut_lua().state_ptr();
        //     ffi::lua_getglobal(lptr, c_ptr!("require"));
        //     ffi::lua_pushstring(lptr, c_ptr!("fiber"));
        //     if ffi::lua_pcall(lptr, 1, 1, 0) == ffi::LUA_ERRRUN {
        //         panic!(
        //             "{:?}",
        //             std::ffi::CStr::from_ptr(ffi::lua_tostring(lptr, -1)),
        //         )
        //     }
        // };
        // fibers.push(
        //     fiber::defer(move || {
        //         v.into_iter().map(|e| e + 1).collect::<Vec<_>>()
        //     })
        // )
    }
    res.push(1);
    let mut registry = LuaTable::registry(lua);
    for fiber_ref in references {
        unsafe {
            ffi::lua_rawgeti(lptr, ffi::LUA_REGISTRYINDEX, fiber_ref);
            ffi::lua_getfield(lptr, -1, c_ptr!("join"));
            ffi::lua_pushvalue(lptr, -1);
            if ffi::lua_pcall(lptr, 2, 1, 0) == ffi::LUA_ERRRUN {
                panic!(
                    "lua_pcall failed: {:?}",
                    CStr::from_ptr(ffi::lua_tostring(lptr, -1)),
                )
            };
            // let ud_ptr = ffi::lua_touserdata(lua.as_lua().state_ptr(), index);

            // if let Some(ud) = TypedUserData::<T>::try_from_ptr_mut(ud_ptr)
            ffi::luaL_unref(lptr, ffi::LUA_REGISTRYINDEX, fiber_ref);
        }
        let mut fiber: LuaTable<_> = registry.get(fiber_ref).unwrap();
        let cur_res: &mut TypedUserData<Vec<i32>> = fiber.method("join", ()).unwrap();
        res.extend(cur_res.take().into_iter().flatten());
    }
    res.push(8);
    assert_eq!(res, vec![1, 2, 3, 4, 5, 6, 7, 8]);
}

