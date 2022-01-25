local vshard = require('vshard')

local function create(key, value)
    local bucket_id = vshard.router.bucket_id(key)
    local data, err = vshard.router.callrw(bucket_id, 'app.storage.api.insert', { key, value })
    return data, err
end

local function get(key)
    local bucket_id = vshard.router.bucket_id(key)
    local data, err = vshard.router.callro(bucket_id, 'app.storage.api.get', { key })
    return data, err
end

local function update(key, value)
    local bucket_id = vshard.router.bucket_id(key)
    local data, err = vshard.router.callro(bucket_id, 'app.storage.api.update', { key, value })
    return data, err
end

local function delete(key)
    local bucket_id = vshard.router.bucket_id(key)
    local data, err = vshard.router.callrw(bucket_id, 'app.storage.api.delete', { key })
    return data, err
end

return {
    create = create,
    get = get,
    update = update,
    delete = delete,
}
