local function stop()
    --
end

local function validate_config(conf_new, conf_old)
    local topology_instances = 0
    for _, replicaset in pairs(conf_new.topology) do
        if type(replicaset) == 'table' then
            for role_name, _ in pairs(replicaset.roles or {}) do
                if role_name == 'topology' then
                    topology_instances = topology_instances + #replicaset.master
                end
            end
        end
    end
    if topology_instances > 1 then
        return false
    end
    return true
end

local function apply_config(conf, opts)
    if opts.is_master then
        --
    end

    --

    return true
end

local function init()
    return true
end

return {
    role_name = 'topology',
    init = init,
    stop = stop,
    validate_config = validate_config,
    apply_config = apply_config,
    dependencies = {'cartridge.roles.vshard-router'}
}
