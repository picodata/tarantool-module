-- role logger

local log = require('log')

local function role_log(role_name) 
    local function role_log_msg(fmtstr, ...) 
        return role_name .. ": " .. fmtstr, ...
    end
    
    return {
        info = function(...) log.info(role_log_msg(...)) end,
        warn = function(...) log.warn(role_log_msg(...)) end,
    } 
end

return {
    role_log = role_log
}
