#!/usr/bin/env tarantool

package.cpath = 'target/debug/?.so;' .. package.cpath

box.cfg{
    listen = 3301,
}

box.once('bootstrap_libinteract_with_lua', function()
    box.schema.func.create('libinteract_with_lua.run', {language = 'C'})
end)

function sum(a, b)
    return a + b
end

box.func['libinteract_with_lua.run']:call()
os.exit(0)
