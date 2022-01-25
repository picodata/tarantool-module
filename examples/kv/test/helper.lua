-- This file is required automatically by luatest.
-- Add common configuration here.

local fio = require('fio')
local t = require('luatest')
local cartridge_helpers = require('cartridge.test-helpers')

local helper = {}

helper.root = fio.dirname(fio.abspath(package.search('init')))
helper.datadir = fio.pathjoin(helper.root, 'tmp', 'db_test')
helper.server_command = fio.pathjoin(helper.root, 'init.lua')

helper.cluster = cartridge_helpers.Cluster:new({
    server_command = helper.server_command,
    datadir = helper.datadir,
    use_vshard = false,
    replicasets = {
        {
            alias = 'api',
            uuid = cartridge_helpers.uuid('a'),
            roles = {'app.roles.custom'},
            servers = {
                { instance_uuid = cartridge_helpers.uuid('a', 1), alias = 'api' },
            },
        },
    }
})

function helper.truncate_space_on_cluster(cluster, space_name)
    assert(cluster ~= nil)
    for _, server in ipairs(cluster.servers) do
        server.net_box:eval([[
            local space_name = ...
            local space = box.space[space_name]
            if space ~= nil and not box.cfg.read_only then
                space:truncate()
            end
        ]], {space_name})
    end
end

function helper.drop_space_on_cluster(cluster, space_name)
    assert(cluster ~= nil)
    for _, server in ipairs(cluster.servers) do
        server.net_box:eval([[
            local space_name = ...
            local space = box.space[space_name]
            if space ~= nil and not box.cfg.read_only then
                space:drop()
            end
        ]], {space_name})
    end
end

function helper.stop_cluster(cluster)
    assert(cluster ~= nil)
    cluster:stop()
    fio.rmtree(cluster.datadir)
end

t.before_suite(function()
    fio.rmtree(helper.datadir)
    fio.mktree(helper.datadir)
    box.cfg({work_dir = helper.datadir})
end)

return helper
