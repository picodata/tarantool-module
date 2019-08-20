#!/usr/bin/env tarantool
-- Sets common rocks paths so that the app can be started from any
-- directory
--
-- By default, when you do require('mymodule'), tarantool looks into
-- the current working directory and whatever is specified in
-- package.path and package.cpath. If you run your app while in the
-- root directory of that app, everything goes fine, but if you try to
-- start your app with "tarantool myapp/init.lua", it will fail to load
-- its modules, and modules from myapp/.rocks.
--
-- This module is a workaround for that behavior. It figures out the
-- path to itself, and then adds its containing directory to standard
-- package.path/package.cpath.
--
-- Usage: put env.lua at the root of your repository, near init.lua.
-- And then in init.lua at the very beginning of file do:
--
-- local script_dir = debug.getinfo(1, "S").source:sub(2):match("(.*/)") or './'
-- dofile(script_dir ..'/env.lua')

local fio = require('fio')

local function get_base_dir()
    return fio.abspath(fio.dirname(arg[0]))
end

local function extend_path(path)
    package.path = package.path .. ';' .. path
end

local function extend_cpath(path)
    package.cpath = package.cpath .. ';' .. path
end

local function set_base_load_paths(base_dir)
    extend_path(base_dir .. '/?.lua')
    extend_path(base_dir .. '/?/init.lua')
    extend_cpath(base_dir .. '/?.dylib')
    extend_cpath(base_dir .. '/?.so')
end

local function set_rocks_load_paths(base_dir)
    extend_path(base_dir..'/.rocks/share/tarantool/?.lua')
    extend_path(base_dir..'/.rocks/share/tarantool/?/init.lua')
    extend_cpath(base_dir..'/.rocks/lib/tarantool/?.dylib')
    extend_cpath(base_dir..'/.rocks/lib/tarantool/?.so')
end

local function set_load_paths(base_dir)
    set_base_load_paths(base_dir)
    set_rocks_load_paths(base_dir)
end

set_load_paths(get_base_dir())
