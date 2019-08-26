
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

local kv_storage = {
    get = function(key)
        local ok, data = pcall(box.space.kv_store.get, box.space.kv_store, key)
        if not ok then
            return nil, data
        end
        return data:tomap({ names_only = true })
    end,
    delete = function(key)
        local ok, data = pcall(box.space.kv_store.delete, box.space.kv_store, key)
        if not ok then
            return nil, data
        end
        return data
    end,
    update = function(key, value)
        local ok, data = pcall(box.space.kv_store.update, box.space.kv_store, { key }, {{ '=', 2, value }})
        if not ok then
            return nil, data
        end
        return data
    end,
    insert = function(key, value)
        local ok, data = pcall(box.space.kv_store.insert, box.space.kv_store, { key, value })
        if not ok then
            return nil, data
        end
        return data
    end,
    get_all_data = function()
        local res = {}
        for _, t in box.space.kv_store:pairs() do
            table.insert(res, t:tomap({names_only=true}))
        end
        return res
    end,
}

local function init(opts)
    rawset(_G, 'kv_storage', kv_storage)
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

        for name, _ in pairs(kv_storage) do
            box.schema.func.create('kv_storage.' .. name, { setuid = true, if_not_exists = true })
            box.schema.user.grant('admin', 'execute', 'function', 'kv_storage.' .. name, { if_not_exists = true })
        end
    end
    return true
end

return {
    role_name = 'storage',
    init = init,
    stop = stop,
    validate_config = validate_config,
    apply_config = apply_config,
    dependencies = {'cartridge.roles.vshard-storage'}
}
