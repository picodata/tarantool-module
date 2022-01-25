local cartridge = require('cartridge')
local storage_api = require('app.storage.api')

local function init(opts) -- luacheck: no unused args
    if opts.is_master then
        local kv_store = box.schema.space.create(
            'kv_store',
            { if_not_exists = true }
        )

        kv_store:format({
            { name = 'key',   type = 'string' },
            { name = 'value', type = '*' },
        })

        kv_store:create_index('primary',
            { type = 'hash', parts = {1, 'string'}, if_not_exists = true }
        )

    end

    rawset(_G, 'app', {storage = {api = storage_api}})
    for name, func in pairs(storage_api) do
        box.schema.func.create('app.storage.api.' .. name, { setuid = true, if_not_exists = true })
        box.schema.user.grant('admin', 'execute', 'function', 'app.storage.api.' .. name, { if_not_exists = true })
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
    role_name = 'app.roles.storage',
    init = init,
    stop = stop,
    validate_config = validate_config,
    apply_config = apply_config,
    dependencies = {'cartridge.roles.vshard-storage'},
}
