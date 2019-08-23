#!/usr/bin/env tarantool

require('strict').on()



-- This is necessary so that we can start init.lua
-- even when we are not in the same directory with it
local script_dir = debug.getinfo(1, "S").source:sub(2):match("(.*/)") or './'
dofile(script_dir ..'/env.lua')

local log = require('log')
local cluster = require('cluster')
local console = require('console')

local work_dir = os.getenv("TARANTOOL_WORK_DIR") or '.'
local instance_name = os.getenv("TARANTOOL_INSTANCE_NAME")
local console_sock = os.getenv("TARANTOOL_CONSOLE_SOCK")
local advertise_uri = os.getenv("TARANTOOL_ADVERTISE_URI")

local http_port = os.getenv("TARANTOOL_HTTP_PORT") or 8081

local ok, err = cluster.cfg({
    alias = instance_name,
    workdir = work_dir,
    advertise_uri = advertise_uri,
    cluster_cookie = 'secret-cluster-cookie',
    bucket_count = 30000,
    http_port = http_port,
    roles = {
        'cluster.roles.vshard-router',
        'cluster.roles.vshard-storage',
        'key-value.key-value',
        'key-value.storage',
        'key-value.topology'
    },
}, {memtx_memory = 128 * 1024 * 1024})

assert(ok, tostring(err))

if console_sock ~= nil then
    console.listen('unix/:' .. console_sock)
end
