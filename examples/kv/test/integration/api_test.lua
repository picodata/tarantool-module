local t = require('luatest')
local g = t.group('integration_api')

local helper = require('test.helper')
local cluster = helper.cluster

g.before_all = function()
    g.cluster = helper.cluster
    g.cluster:start()
end

g.after_all = function()
    helper.stop_cluster(g.cluster)
end

g.before_each = function()
    -- helper.truncate_space_on_cluster(g.cluster, 'Set your space name here')
end

g.test_sample = function()
    local server = cluster.main_server
    local response = server:http_request('post', '/admin/api', {json = {query = '{ cluster { self { alias } } }'}})
    t.assert_equals(response.json, {data = { cluster = { self = { alias = 'api' } } }})
    t.assert_equals(server.net_box:eval('return box.cfg.memtx_dir'), server.workdir)
end

g.test_metrics = function()
    local server = cluster.main_server
    local response = server:http_request('get', '/metrics')
    t.assert_equals(response.status, 200)
    t.assert_equals(response.reason, "Ok")
end
