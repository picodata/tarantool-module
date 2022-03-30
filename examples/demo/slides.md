----
# разработка под tarantool на rust












crates.io/crates/tarantool
---
----
# зачем?

luajit:
  - супер простой
    - всего 22 ключевых слова
    - boolean, number, string, table, function

  - мощный
    - metatables
    - userdata
    - luajit ffi

  - эффективный
    - во многих случаях сравним с С/С++
---
----
# зачем?                                       (1/2)

rust:
  - строгая compile-time типизация
    - никаких "attempt to call nil value"
    - 

  - memory safety
    - защита от утечек без gc
    - защита от race condition

  - эффективность
    - сравнима с С/С++

---
----
# зачем?                                       (2/2)

rust:
  - подробная документация
    - doc.rust-lang.org/std
    - docs.rs

  - поддержка ide (rls)
    - autocomplete
    - go to definition
    - документация

  - быстро растущее community
    - много сторонних модулей
    - 0.5% вопросов на stackoverflow c тэгом [rust]
---
----
# как?

tarantool:

  - нативные хранимые процедуры
```lua
      box.schema.func.create('foo', {language = 'C'})
```

  - luajit-ffi нативные динамические библиотеки
```lua
      ffi = require 'ffi'; ffi.cdef [[ void bar(void); ]]
      foo = ffi.load('/path/to/libfoo.so')
      foo.bar()
```
---
----
# tarantool-module                             (1/3)

создаём cargo проект
```bash
cargo new --lib tnt-demo
# tnt-demo
# ├── Cargo.toml
# └── src
#     └── lib.rs
```





---
----
# tarantool-module                `Cargo.toml` (2/3)

прописываем конфигурацию в Cargo.toml
```toml
[dependencies]
tarantool = { version = "0.6", features = ["schema"] }
thiserror = "1.0"

[lib]
crate-type = ["cdylib"]
```




---
----
# tarantool-module                    `lib.rs` (3/3)
```rust
use tarantool::space::{Space, Field};

#[tarantool::proc]
fn create(space_name: String) -> Result<(), MyError> {
    let space = Space::builder(&space_name)
        .field(Field::unsigned("key"))
        .field(Field::string("value"))
        .create()?;

    space.index_builder("primary")
        .part("key")
        .create()?;

    Ok(())
}
```
---
----
# tarantool-module                    `lib.rs` (3/3)
```rust
#[tarantool::proc]
fn insert(space_name: String, values: Vec<(usize, String)>)
  -> Result<(), MyError>
{
    let mut space = Space::find(&space_name)
      .ok_or_else(|| MyError::SpaceNotFound(space_name))?;

    for (key, value) in values {
        space.insert(&(key, value))?;
    }
    Ok(())
}
```
---
----
# tarantool-module                    `lib.rs` (3/3)
```rust
use tarantool::index::IteratorType;

#[tarantool::proc]
fn get(space_name: String, key: usize) -> Result<String, MyError> {
    let space = Space::find(&space_name)
      .ok_or_else(|| MyError::SpaceNotFound(space_name))?;

    if let Some(row) = space.select(IteratorType::Eq, &[key])?.next() {
        let (_, value) = row.into_struct::<(usize, String)>()?;
        Ok(value)
    } else {
        Err(MyError::KeyNotFound(key))
    }
}
```

----
# tarantool-module                    `lib.rs` (3/3)
```rust
#[derive(Debug, thiserror::Error)]
enum MyError {
    #[error("space '{0}' not found")]
    SpaceNotFound(String),

    #[error("key '{0}' not found")]
    KeyNotFound(usize),

    #[error("tarantool error: {0}")]
    Tarantool(#[from] tarantool::error::Error),
}
```





---
----
# tarantool-module                  `demo.lua` (*/*)

demo.lua:
```lua
box.cfg {}

box.schema.func.create('tnt-demo.create', { language = 'C' })
box.schema.func.create('tnt-demo.insert', { language = 'C' })
box.schema.func.create('tnt-demo.get',    { language = 'C' })

space_name = 'demo_space'
box.func['tnt-demo.create']:call { space_name }
box.func['tnt-demo.insert']:call { space_name, { {1, 'foo'}, {2, 'bar'} } }
assert(box.func['tnt_demo.get']:call { space_name, 1 } == 'foo')
assert(box.func['tnt_demo.get']:call { space_name, 2 } == 'bar')

os.exit(0)
```




---
----
# tarantool-module                             (*/*)

запускаем
```bash
cargo build

LUA_CPATH=target/debug/lib?.so tarantool demo.lua
```




---
----
# как














