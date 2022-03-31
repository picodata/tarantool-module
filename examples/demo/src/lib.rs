use tarantool::{
    index::{IndexType, IteratorType},
    space::{Space, Field},
    tlua::{self, Call},
};

use std::io::Write;

#[derive(tlua::Push)]
struct Cfg {
    listen: String,
    wal_dir: String,
    memtx_dir: String,
}

#[no_mangle]
pub extern "C" fn setup() {
    let lua = tarantool::lua_state();
    let cfg: tlua::Callable<_> = lua.eval("return box.cfg").unwrap();
    let () = cfg.call_with(&Cfg {
        listen: "3301".into(),
        wal_dir: "tmp".into(),
        memtx_dir: "tmp".into(),
    }).unwrap();

    let mut space = Space::builder("test_space")
        .field(Field::unsigned("id"))
        .field(Field::string("text"))
        .create()
        .unwrap();

    space.index_builder("primary")
        .index_type(IndexType::Tree)
        .part("id")
        .create()
        .unwrap();

    space.insert(&(1, "foo".to_string())).unwrap();
    space.insert(&(2, "bar".to_string())).unwrap();
    space.insert(&(3, "baz".to_string())).unwrap();

    lua.exec("box.schema.func.create('demo.example', { language = 'C' })").unwrap();
    lua.exec("box.schema.func.create('demo.insert', { language = 'C' })").unwrap();
}

#[derive(serde::Serialize)]
struct Out {
    concat: String,
    sum: usize,
}

#[tarantool::proc]
fn insert(space_name: String, values: Vec<(usize, String)>) -> Result<(), Error> {
    let mut space = Space::find(&space_name)
        .ok_or_else(|| Error::SpaceNotFound(space_name))?;

    for (id, text) in values {
        space.insert(&(id, text))?;
    }

    Ok(())
}

#[tarantool::proc]
fn example(space_name: String, min: usize, max: usize) -> Result<Out, Error> {
    let mut buf = Vec::with_capacity(1024);
    let mut sum = 0;

    let space = Space::find(&space_name)
        .ok_or_else(|| Error::SpaceNotFound(space_name))?;
    for row in space.select(IteratorType::GE, &[min])? {
        let (id, text) = row.into_struct::<(usize, String)>()?;
        if id > max {
            break
        }
        sum += id;
        write!(buf, "{text}")?;
    }
    Ok(Out { concat: String::from_utf8(buf)?, sum })
}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("space '{0}' not found")]
    SpaceNotFound(String),
    #[error("tarantool error: {0}")]
    Tarantool(#[from] tarantool::error::Error),
    #[error("utf8 error: {0}")]
    FromUtf8(#[from] std::string::FromUtf8Error),
    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),
}

use tarantool::tlua;

// #[no_mangle]
// fn luaopen_demo()
