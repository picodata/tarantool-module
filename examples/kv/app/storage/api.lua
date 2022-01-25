local function insert(key, value)
    local ok, data = pcall(box.space.kv_store.insert, box.space.kv_store, { key, value })
    if not ok then
        return nil, data
    end
    return data
end

local function get(key)
    local ok, data = pcall(box.space.kv_store.get, box.space.kv_store, key)
    if not ok then
        return nil, data
    end
    return data:tomap({ names_only = true })
end

local function update(key, value)
    local ok, data = pcall(box.space.kv_store.update, box.space.kv_store, { key }, {{ '=', 2, value }})
    if not ok then
        return nil, data
    end
    return data
end

local function delete(key)
    local ok, data = pcall(box.space.kv_store.delete, box.space.kv_store, key)
    if not ok then
        return nil, data
    end
    return data
end

return {
    insert = insert,
    get = get,
    update = update, 
    delete = delete
}
