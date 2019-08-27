#!/usr/bin/env tarantool

require('strict').on()



-- This is necessary so that we can start init.lua
-- even when we are not in the same directory with it
local script_dir = debug.getinfo(1, "S").source:sub(2):match("(.*/)") or './'
dofile(script_dir ..'/env.lua')

local log = require('log')
local cartridge = require('cartridge')
local console = require('console')
local fiber = require('fiber')

local work_dir = os.getenv("TARANTOOL_WORK_DIR") or '.'
local instance_name = os.getenv("TARANTOOL_INSTANCE_NAME")
local console_sock = os.getenv("TARANTOOL_CONSOLE_SOCK")
local advertise_uri = os.getenv("TARANTOOL_ADVERTISE_URI")
local memtx_memory = tonumber(os.getenv("TARANTOOL_MEMTX_MEMORY")) or (128 * 1024 * 1024)

local http_port = os.getenv("TARANTOOL_HTTP_PORT") or 8081

local fiber = require('fiber')
local log = require('log')

local http_client = require('http.client')
local http_server = require('http.server')

local function resolve_uri(uri, timeout)
    if not uri then
        return nil, "Pass URI in the next format: uri:port"
    end

    timeout = timeout or 10
    -- local uri, port = uri:match("(^.*)%:(.*)")
    local resolved = false

    local server_options = {
        log_errors = true,
        log_requests = log.debug
    }

    local srv = http_server.new("0.0.0.0", "3301", server_options)
    srv:route({ path = '/dns_resolver', method = 'GET' }, function(_) return { status = 200, text = 'Success' } end)
    srv:start()

    local time = 0
    while time < timeout do
        local resp = http_client.get(uri .. '/dns_resolver')
        if resp.status ~= nil and resp.status == 200 then
            resolved = true
            break
        else
            print('Not resolved yet')
        end

        fiber.sleep(1)
        time = time + 1
    end
    srv:stop()

    return resolved
end

local t = resolve_uri(advertise_uri, 50)
if not t then os.exit(1) end
fiber.sleep(5)

local ok, err = cluster.cfg({
    alias = instance_name,
    workdir = work_dir,
    advertise_uri = advertise_uri,
    cluster_cookie = 'secret-cluster-cookie',
    bucket_count = 30000,
    http_port = http_port,
    roles = {
        'cartridge.roles.vshard-router',
        'cartridge.roles.vshard-storage',
        'key-value.key-value',
        'key-value.storage',
    },
}, {memtx_memory = memtx_memory})

assert(ok, tostring(err))

if console_sock ~= nil then
    console.listen('unix/:' .. console_sock)
end
