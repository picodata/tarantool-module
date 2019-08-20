local log = require('log')
local json = require('json')

local logger = require('log')
vshard = require('vshard')

local function invalid_body(req, func_name,  msg)
    local resp = req:render{json = { info = msg }}
    resp.status = 400
    logger.info("%s(%d) invalid body: %s", func_name, resp.status, req.body)
    return resp
end

local function internal_error(req, func_name,  msg)
    local resp = req:render{json = { info = msg }}
    resp.status = 500
    logger.info("%s(%d) internal_error: %s", func_name, resp.status, req.body)
    return resp
end

local function read_json(request)
    local status, body = pcall(function() return request:json() end)
    logger.info("pcall(request:json()): %s %s", status, body)
    logger.info("type of body: %s", type(body))
    return body
end

local function create(req)
    local body = read_json(req)

    if type(body) == 'string' then
        return invalid_body(req, 'create', 'invlid json')
    end

    if body['key'] == nil or body['value'] == nil then
        return invalid_body(req, 'create', 'missing value or key')
    end

    local key = body['key']

    local bucket_id = vshard.router.bucket_id(key)
    local data, err = vshard.router.callrw(bucket_id, 'kv_storage.insert', { key, body['value'] })

    if data == nil then
        if string.find(err, 'Duplicate key') ~= nil then
            local resp = req:render{json = { info = "duplicate keys" }}
            resp.status = 409
            logger.info("create(%d) conflict keys: %s", resp.status, key)
            return resp
        else
            logger.info(err)
            return internal_error(req, 'create', 'insertion failed')
        end
    end

    local resp = req:render{json = { info = "Successfully created" }}
    resp.status = 201

    logger.info("create(%d) key: %s", resp.status, key)

    return resp
end

local function delete(req)
    local key = req:stash('key')

    local bucket_id = vshard.router.bucket_id(key)
    local data, err = vshard.router.callrw(bucket_id, 'kv_storage.delete', { key })

    if data == nil then
        local resp = req:render{json = { info = "Key doesn't exist", msg = err and err.msg }}
        resp.status = 404
        return resp
    end

    local resp = req:render{json = { info = "Successfully deleted" }}
    resp.status = 200
    return resp
end

local function get_tuple(req)
    local key = req:stash('key')

    local bucket_id = vshard.router.bucket_id(key)
    local data, err = vshard.router.callro(bucket_id, 'kv_storage.get', { key })

    if data == nil  then
        local resp = req:render{json = { info = "Key doesn't exist", msg = err and err.msg }}
        resp.status = 404
        return resp
    end

    logger.info("GET(key: %s)" , key)
    local resp = req:render{ json = { key = data.key, value = data.value }}
    resp.status = 200

    return resp
end

local function update(req)
    local body = read_json(req)

    if type(body) == 'string' then
        return invalid_body(req, 'update', 'invlid json')
    end

    if body['value'] == nil then
        return invalid_body(req, 'update', 'missing value')
    end

    local key = req:stash('key')

    if key == nil then
        local resp = req:render{json = { info = 'Key must be provided' }}
        resp.status = 400
        logger.info("update(%d) invalid key: '%s'", resp.status, key)
        return resp
    end

    local bucket_id = vshard.router.bucket_id(key)
    local data, err = vshard.router.callro(bucket_id, 'kv_storage.update', { key, body['value'] })

    if data == nil then
        local resp = req:render{json = { info = "Key doesn't exist", msg = err and err.msg }}
        resp.status = 404
        return resp
    end

    logger.info("PUT(key: %s): value: %s" , key, body['value'])
    local resp = req:render{json = { info = "Successfully updated" }}
    resp.status = 200
    return resp
end

local function get_all_kv(req)
    local results = {}
    for _, replicaset in pairs(vshard.router.routeall()) do
        local res = replicaset:callro('kv_storage.get_all_data')
        for _, tuple in ipairs(res) do
            table.insert(results, tuple)
        end
    end

    local resp = req:render{json = { store = results }}
    resp.status = 200
    logger.info("get_all_kv(200)")
    return resp
end


return {
    get_tuple = get_tuple,
    create = create,
    delete = delete,
    update = update,
    get_all = get_all_kv
}
