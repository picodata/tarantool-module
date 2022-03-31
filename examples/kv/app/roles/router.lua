local cartridge = require('cartridge')
local router_api = require('app.router.api')

local function handle_create(req)
    local status, body = pcall(function() return req:json() end)

    if type(body) == 'string' then
        local resp = req:render{json = { info = msg }}
        resp.status = 500
        return resp
    end

    if body['key'] == nil then
        local resp = req:render{json = { info = msg }}
        resp.status = 400
        return resp
    end

    if body['value'] == nil then
        local resp = req:render{json = { info = msg }}
        resp.status = 400
        return resp
    end

    local data, err = router_api.create(body['key'], body['value'])
    if err ~= nil then
        local resp = req:render({json = {error = err}})
        resp.status = 500
        return resp
    end

    local resp = req:render({json = data})
    resp.status = 201

    return resp
end

local function handle_get(req)
    local key = req:stash('key')
    local data, err = router_api.get(key)

    if err ~= nil then
        local resp = req:render{json = { info = "Key doesn't exist", msg = err and err.msg }}
        resp.status = 400
        return resp
    end

    return req:render({json = data['value']})
end

local function handle_update(req)
    local key = req:stash('key')
    local _, value = pcall(function() return req:json() end)

    if value == nil then
        local resp = req:render{json = { info = "Value cannot be empty" }}
        resp.status = 500
        return resp
    end

    local data, err = router_api.update(key, value)
    if err ~= nil then
        local resp = req:render({json = {error = err}})
        resp.status = 500
        return resp
    end

    local resp = req:render({json = data})
    resp.status = 201

    return resp
end

local function handle_delete(req)
    local key = req:stash('key')

    local data, err = router_api.delete(key)

    if err ~= nil then
        local resp = req:render{json = { info = "Key doesn't exist", msg = err and err.msg }}
        resp.status = 400
        return resp
    end

    local resp = req:render{json = { info = "Successfully deleted" }}
    resp.status = 200
    return resp
end

local function init(opts) -- luacheck: no unused args
    local httpd = cartridge.service_get('httpd')
    if httpd ~= nil then
        httpd:route({ path = '/kv/', method = 'POST', public = true }, handle_create)
        httpd:route({ path = '/kv/:key', method = 'GET', public = true }, handle_get)
        httpd:route({ path = '/kv/:key', method = 'DELETE', public = true }, handle_delete)
        httpd:route({ path = '/kv/:key', method = 'PUT  ', public = true }, handle_update)
    end

    rawset(_G, 'app', {router = {api = router_api}})
    for name, _ in pairs(router_api) do
        box.schema.func.create('app.router.api.' .. name, { setuid = true, if_not_exists = true })
        box.schema.user.grant('admin', 'execute', 'function', 'app.router.api.' .. name, { if_not_exists = true })
    end

    return true
end

local function stop()
    return true
end

local function validate_config(conf_new, conf_old) -- luacheck: no unused args
    return true
end

local function apply_config(conf, opts) -- luacheck: no unused args
    -- if opts.is_master then
    -- end

    return true
end

return {
    role_name = 'app.roles.router',
    init = init,
    stop = stop,
    validate_config = validate_config,
    apply_config = apply_config,
    dependencies = {'cartridge.roles.vshard-router'},
}
