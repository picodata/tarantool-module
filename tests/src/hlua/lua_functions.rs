
use tarantool::hlua::{
    LuaError,
    LuaFunction,
    LuaTable,    Lua,
};
use std::io::Read;

pub fn basic() {
    let mut lua = crate::hlua::global();
    let mut f = LuaFunction::load(&mut lua, "return 5;").unwrap();
    let val: i32 = f.call().unwrap();
    assert_eq!(val, 5);
}

pub fn args() {
    let mut lua = crate::hlua::global();
    lua.execute::<()>("function foo(a) return a * 5 end").unwrap();
    let val: i32 = lua.get::<LuaFunction<_>, _>("foo").unwrap().call_with_args(3).unwrap();
    assert_eq!(val, 15);
}

pub fn args_in_order() {
    let mut lua = crate::hlua::global();
    lua.execute::<()>("function foo(a, b) return a - b end").unwrap();
    let val: i32 = lua.get::<LuaFunction<_>, _>("foo").unwrap().call_with_args((5, 3)).unwrap();
    assert_eq!(val, 2);
}

pub fn syntax_error() {
    let mut lua = crate::hlua::global();
    match LuaFunction::load(&mut lua, "azerazer") {
        Err(LuaError::SyntaxError(_)) => (),
        _ => panic!(),
    };
}

pub fn execution_error() {
    let mut lua = crate::hlua::global();
    let mut f = LuaFunction::load(&mut lua, "return a:hello()").unwrap();
    match f.call::<()>() {
        Err(LuaError::ExecutionError(_)) => (),
        _ => panic!(),
    };
}

pub fn check_types() {
    let mut lua = crate::hlua::global();
    let mut f = LuaFunction::load(&mut lua, "return 12").unwrap();
    let err = f.call::<bool>().unwrap_err();
    match err {
        LuaError::WrongType{ref rust_expected, ref lua_actual} => {
            assert_eq!(rust_expected, "bool");
            assert_eq!(lua_actual, "number");
        },
        v => panic!("{}", v),
    };
    let err_str : String = err.to_string();
    assert_eq!(
        err_str,
        "Wrong type returned by Lua: bool expected, got number"
    );

    assert_eq!(f.call::<i32>().unwrap(), 12i32);
    assert_eq!(f.call::<f32>().unwrap(), 12f32);
    assert_eq!(f.call::<f64>().unwrap(), 12f64);
    assert_eq!(f.call::<String>().unwrap(), "12".to_string());
}

pub fn call_and_read_table() {
    let mut lua = crate::hlua::global();
    let mut f = LuaFunction::load(&mut lua, "return {1, 2, 3};").unwrap();
    let mut val: LuaTable<_> = f.call().unwrap();
    assert_eq!(val.get::<u8, _, _>(2).unwrap(), 2);
}

pub fn lua_function_returns_function() {
    let mut lua = crate::hlua::global();
    lua.execute::<()>("function foo() return 5 end").unwrap();
    let mut bar = LuaFunction::load(&mut lua, "return foo;").unwrap();
    let mut foo: LuaFunction<_> = bar.call().unwrap();
    let val: i32 = foo.call().unwrap();
    assert_eq!(val, 5);
}

pub fn execute_from_reader_errors_if_cant_read() {
    struct Reader { }

    impl Read for Reader {
        fn read(&mut self, _: &mut [u8]) -> ::std::io::Result<usize> {
            use std::io::{Error, ErrorKind};
            Err(Error::new(ErrorKind::Other, "oh no!"))
        }
    }

    let mut lua = crate::hlua::global();
    let reader = Reader { };
    let res: Result<(), _> = lua.execute_from_reader(reader);
    match res {
        Ok(_) => panic!("Reading succeded"),
        Err(LuaError::ReadError(e)) => { assert_eq!("oh no!", e.to_string()) },
        Err(_) => panic!("Unexpected error happened"),
    }
}

pub fn test_list_error()  {
    let mut err = LuaError::NoError;
    assert_eq!( err.is_collection(), false );
    err.add( &LuaError::WrongType{ rust_expected : "Type1".to_string(), lua_actual : "LType1".to_string()  } );
    assert_eq!( err.is_collection(), false );
    err.add( &LuaError::WrongType{ rust_expected : "Type2".to_string(), lua_actual : "LType2".to_string()  } );
    assert_eq!( err.is_collection(), true );
    err.add( &LuaError::WrongType{ rust_expected : "Type3".to_string(), lua_actual : "LType3".to_string()  } );
    err.add( &LuaError::WrongType{ rust_expected : "Type4".to_string(), lua_actual : "LType4".to_string()  } );
    err.add( &LuaError::WrongType{ rust_expected : "Type5".to_string(), lua_actual : "LType5".to_string()  } );
    assert!( err.is_collection() );
    assert_eq!( err.get_collection().len(), 5 );
    static EXPECTED : &'static [&'static str] = &[
        "Type1",
        "Type2",
        "Type3",
        "Type4",
        "Type5",
    ];
    static ACTUAL : &'static [&'static str] = &[
        "LType1",
        "LType2",
        "LType3",
        "LType4",
        "LType5",
    ];
    let mut counter = 0;
    for elem in err.iter() {
        if let LuaError::WrongType{ rust_expected : exp_var, lua_actual : act_val } = elem {
            assert_eq!( exp_var, EXPECTED[counter] );
            assert_eq!( act_val, ACTUAL[counter] );
        } else {
            assert!( false );
        }
        counter = counter + 1;
    }
}

pub fn test_display_error()  {
    let err1 = LuaError::NoError;
    let err2 = LuaError::SyntaxError("22".to_string());
    let err3 = LuaError::ExecutionError("333".to_string());
    let _file = std::fs::File::open("/aaa/f.txt/NKOhm2P1W3ivOfvffLdh6mkj0MiUcKJR0977VZoS");
    let err4 = LuaError::ReadError(std::sync::Arc::new(std::io::Error::last_os_error()));
    let err5 = LuaError::WrongType{ rust_expected: "4444".to_string(), lua_actual: "aaaa".to_string() };
    assert_eq!( format!("{}",err1), "No Error".to_string() );
    assert_eq!( format!("{}",err2), "Syntax error: 22".to_string() );
    assert_eq!( format!("{}",err3), "Execution error: 333".to_string() );
    assert_eq!( format!("{}",err4), "Read error: No such file or directory (os error 2)".to_string() );
    assert_eq!( format!("{}",err5), "Wrong type returned by Lua: 4444 expected, got aaaa".to_string() );

    let mut erlist = LuaError::NoError;
    erlist.add( &LuaError::SyntaxError("1".to_string()) );
    assert_eq!( format!("{}",erlist), "Syntax error: 1".to_string() );
    erlist.add( &LuaError::SyntaxError("2".to_string()) );
    assert_eq!( format!("{}",erlist), "Syntax error: 2".to_string() );
    erlist.add( &LuaError::SyntaxError("3".to_string()) );
    assert_eq!( format!("{}",erlist), "Syntax error: 3".to_string() );
}

macro_rules! for_iteration_test {
    ($erlist:expr, $EXPECTED:expr, $additional_action:expr) => {
        let mut counter = 0;
        for localerr in $erlist.iter() {
            assert!( counter < $EXPECTED.len() );
            if counter >= $EXPECTED.len() {break;}
            assert_eq!( format!("{}", localerr) , $EXPECTED[ counter ].to_string() );
            counter = counter + 1;
            additional_action;
        }
    };
}

pub fn test_error_iterations()  {
    let mut erlist = LuaError::NoError;
    assert_eq!( erlist.is_collection(), false );
    erlist.add( &LuaError::SyntaxError("1".to_string()) );
    assert_eq!( erlist.is_collection(), false );
    assert_eq!( format!("{}",erlist), "Syntax error: 1".to_string() );
    erlist.add( &LuaError::SyntaxError("2".to_string()) );
    assert_eq!( erlist.is_collection(), true );
    assert_eq!( format!("{}",erlist), "Syntax error: 2".to_string() );
    erlist.add( &LuaError::SyntaxError("3".to_string()) );
    assert_eq!( erlist.is_collection(), true );
    assert_eq!( format!("{}",erlist), "Syntax error: 3".to_string() );

    let mut it = erlist.iter();
    let a = it.next();
    assert!( a.is_some() );
    assert_eq!( format!("{}",a.unwrap()), "Syntax error: 1".to_string() );
    let b = it.next();
    assert!( b.is_some() );
    assert_eq!( format!("{}",b.unwrap()), "Syntax error: 2".to_string() );
    let c = it.next();
    assert!( c.is_some() );
    assert_eq!( format!("{}",c.unwrap()), "Syntax error: 3".to_string() );
    let d = it.next();
    assert!( d.is_none() );
    let e = it.next();
    assert!( e.is_none() );

    static EXPECTED : &'static [&'static str] = &[
        "Syntax error: 1",
        "Syntax error: 2",
        "Syntax error: 3",
    ];
    for_iteration_test!( erlist, EXPECTED, {});

    let erlist = LuaError::NoError;
    static EXPECTED2 : &'static [&'static str] = &[
        "No Error",
    ];
    for_iteration_test!( erlist, EXPECTED2, {});

    let erlist = LuaError::ExecutionError("hello".to_string());
    static EXPECTED3 : &'static [&'static str] = &[
        "Execution error: hello",
    ];
    for_iteration_test!( erlist, EXPECTED3, {});

    let erlist = LuaError::NoError;
    let mut it = erlist.iter();
    let a = it.next();
    let b = it.next();
    assert!( b.is_none() );
    assert!( a.is_some() );
    assert_eq!( format!("{}",a.unwrap()), "No Error".to_string() );

    let erlist = LuaError::ExecutionError("exec".to_string());
    let mut it = erlist.iter();
    let a = it.next();
    let b = it.next();
    assert!( b.is_none() );
    assert!( a.is_some() );
    assert_eq!( format!("{}",a.unwrap()), "Execution error: exec".to_string() );
}