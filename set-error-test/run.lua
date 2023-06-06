local fio = require('fio')
local json = require('json')
local log = require('log')

local tmpdir = fio.tempdir()

box.cfg{
    log_level = 'verbose',
    listen = 3301,
    wal_mode = 'none',
    memtx_dir = tmpdir,
}

fio.rmtree(tmpdir)

-- Init test database
box.once('bootstrap_tests', function()
    box.schema.user.grant('guest', 'read,write,execute,create,drop', 'universe')

    box.schema.func.create('test_stored_proc')
end)

function test_stored_proc(a, b)
    return a + b
end

function target_dir()
    if rawget(_G, '_target_dir') == nil then
        local data = io.popen('cargo metadata --format-version 1'):read('*l')
        rawset(_G, '_target_dir', json.decode(data).target_directory)
    end
    return _target_dir
end

function build_mode()
    local build_mode_env = os.getenv('TARANTOOL_MODULE_BUILD_MODE')
    if not build_mode_env then
        build_mode_env = 'debug'
    end
    return build_mode_env
end

-- Add test runner library location to lua search path
package.cpath = string.format(
    '%s/%s/?.so;%s/%s/?.dylib;%s',
    target_dir(), build_mode(),
    target_dir(), build_mode(),
    package.cpath
)

box.schema.func.create('libset_error_test.loop_set_error', {language = 'C'})
box.schema.func.create('libset_error_test.leak_mem', {language = 'C'})

box.func['libset_error_test.loop_set_error']:call()
-- box.func['libset_error_test.leak_mem']:call()
os.exit(0)
