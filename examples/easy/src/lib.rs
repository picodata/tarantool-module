use tarantool::proc;
use tarantool::tlua;
use tarantool::tlua::function_type;

#[proc]
fn easy() {
    println!("hello world");
}

#[proc]
fn easy2() {
    println!("hello world -- easy2");
}

#[derive(tlua::PushInto)]
struct Mod {
    foo: tlua::function_type![ () -> String ],
    divmod: tlua::function_type![ (i32, i32) -> (i32, i32) ],
}

#[tarantool::tlua::function(tlua = "tarantool::tlua")]
fn luaopen_easy(wtf: String) -> Mod {
    // Tarantool calls this function upon require("easy")
    println!("easy module loaded: {wtf:?}");
    Mod {
        foo: tlua::Function::new(|| "howdy sailor".into()),
        divmod: tlua::Function::new(|a, b| { (a / b, a % b) })
    }
}

#[tarantool::tlua::function(tlua = "tlua")]
fn get_i<L: tlua::AsLua>(t: tlua::LuaTable<L>, i: i32) -> String {
    t.get(i).unwrap()
}
