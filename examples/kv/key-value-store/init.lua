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

local log = require('log')

--- [HACK] Probing node via membership
local function dns_resolver(opts)
    opts = opts or {}
    opts.timeout = opts.timeout or 100

    local membership = require('membership')

    local hostname, port = advertise_uri:match("^(.*)%:(.*)")
    local ok, err = membership.init(hostname, tonumber(port))
    if not ok then
        log.error("[dns_resolver] Can't init a membership. Error: %s", err)
        os.exit(1)
    end

    membership.set_encryption_key('test')
    membership.set_payload('alias', '__' .. advertise_uri)

    local time = 0
    local resolved = false
    while time < opts.timeout do
        local ok = membership.probe_uri(membership.myself().uri)
        if ok then
            resolved = true
            break
        end

        log.info("DNS resolution has been failed. Trying to probe it again...")
        fiber.sleep(1)
        time = time + 1
    end

    membership.leave()
    if not resolved then
        return false
    end

    return true
end

local resolved = dns_resolver({ timeout = 60 })
if not resolved then
    log.error("[dns_resolver] Instance has not been resolved")
    os.exit(1)
end
--- End of hack

local ok, err = cartridge.cfg({
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
