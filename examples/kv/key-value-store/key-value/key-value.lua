#!/usr/bin/env tarantool

local cartridge = require('cartridge')
local kv_api = require('key-value.api')

local function init(_)
    return true
end

local httpd = cartridge.service_get('httpd')
if httpd ~= nil then
    httpd:route({ path = '/kv', method = 'POST', public = true }, kv_api.create)
    httpd:route({ path = '/kv_dump/', method = 'GET', public = true }, kv_api.get_all)
    httpd:route({ path = '/kv/:key', method = 'DELETE', public = true }, kv_api.delete)
    httpd:route({ path = '/kv/:key', method = 'GET', public = true }, kv_api.get_tuple)
    httpd:route({ path = '/kv/:key', method = 'PUT', public = true }, kv_api.update)
end

local function stop()
    --
end

local function validate_config(conf_new, conf_old)
    --

    return true
end

local function apply_config(conf, opts)
    if opts.is_master then
        --
    end

    --

    return true
end

return {
    role_name = 'router',
    init = init,
    stop = stop,
    validate_config = validate_config,
    apply_config = apply_config,
    dependencies = {'cartridge.roles.vshard-router'}
}
