## Ffi package

This is the package responsible for establishing clear communication contracts between tarantool C-API and its Rust counter parts.

To make our life easier the package uses [bindgen]() utility to automatically generate definitions on the Rust side.

The process to update those generated items is to checkout matching versions of picodata's tarantool submodule, build tarantool. And then launch following bindgen command to perform the generation:
<!-- 
    --raw-line 'use va_list::VaList;' \
    --raw-line 'pub type va_list = VaList;' \
    --blocklist-item va_list \


    --allowlist-file netdb.h \


        --blocklist-item va_list \
    --blocklist-item __builtin_va_list \
    --blocklist-item __va_list_tag \

        --raw-line 'use ::va_list::VaList;' \
    --raw-line 'pub type va_list = VaList;' \
    --raw-line 'pub type __va_list_tag = VaList;' \
-->

```bash
export MODULE=target/debug/build/tarantool-sys/tarantool-prefix/include/tarantool/module.h
bindgen $MODULE --no-doc-comments --allowlist-file $MODULE \
    --blocklist-item addrinfo \
    --blocklist-item lua_State \
    --blocklist-item coio_call \
    --blocklist-item fiber_func \
    --blocklist-item tuple \
    --blocklist-item box_tuple_t \
    --blocklist-item log_write_flightrec \
    --blocklist-item fiber_get_ctx \
    --blocklist-item fiber_set_ctx \
    --raw-line '#![allow(non_camel_case_types)]' \
    --raw-line '#![allow(non_upper_case_globals)]' \
    --raw-line '#![allow(non_snake_case)]' \
    --raw-line 'use libc::*;' \
    --raw-line 'use tlua::ffi::lua_State;' \
    --raw-line 'use super::manual::FiberFunc;' \
    --raw-line 'type fiber_func = FiberFunc;' \
    --raw-line 'use super::manual::BoxTuple;' \
    --raw-line 'type tuple = BoxTuple;' \
    --raw-line 'type box_tuple_t = tuple;' \
    --blocklist-item PACKAGE_VERSION_MAJOR \
    --blocklist-item PACKAGE_VERSION_MINOR \
    --blocklist-item PACKAGE_VERSION_PATCH \
    --blocklist-item PACKAGE_VERSION \
    --blocklist-item SYSCONF_DIR \
    --blocklist-item INSTALL_PREFIX \
    --blocklist-item BUILD_TYPE \
    --blocklist-item BUILD_INFO \
    --blocklist-item BUILD_OPTIONS \
    --blocklist-item COMPILER_INFO \
    --blocklist-item TARANTOOL_C_FLAGS \
    --blocklist-item TARANTOOL_CXX_FLAGS \
    --blocklist-item MODULE_LIBDIR \
    --blocklist-item MODULE_LUADIR \
    --blocklist-item MODULE_INCLUDEDIR \
    --blocklist-item MODULE_LUAPATH \
    --blocklist-item MODULE_LIBPATH \
    --blocklist-item MODULE_LIBSUFFIX \
    --blocklist-item BOX_DECIMAL_STRING_BUFFER_SIZE \
 > tarantool/tarantool/src/ffi/bindings/gen.rs
```

Note: currently we use only addrinfo from `use libc::*;`

This is expected to be run from picodata repository root.