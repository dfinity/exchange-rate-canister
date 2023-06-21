-- Setup a table to be used as a module by nginx.
local _M = {}

-- This function takes the exchange's name, the requested uri and params
-- and creates a path to where the JSON file is expected to be.
function _M.route(exchange_name, uri, params, filetype)
    local param_keys = {}
    -- Populate the param_keys table with the key field of params.
    for k in pairs(params) do table.insert(param_keys, k) end
    -- Sort parameter keys to ensure uniformity with Rust generator.
    table.sort(param_keys)

    -- Build up the filename from the sorted parameter keys.
    local filename = ""
    for _, key in ipairs(param_keys) do
        filename = filename .. key .. "_" .. params[key] .. "_"
    end
    -- Trim the last underscore from filename,
    -- prepend the exchange name, the URI, and append the file extension.
    local path =  "/" .. exchange_name .. uri .. "/" .. filename:sub(1, -2) .. "." .. filetype

    -- Pass the path back to the caller.
    return path
end

-- Expose the module to nginx.
return _M
