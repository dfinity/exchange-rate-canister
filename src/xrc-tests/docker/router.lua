local _M = {}

function _M.route(exchange_name, uri, params)
    local param_keys = {}
    -- Populate the param keys table
    for k in pairs(params) do table.insert(param_keys, k) end
    -- Sort param keys
    table.sort(param_keys)

    -- Build up the filename
    local filename = ""
    for _, key in ipairs(param_keys) do
        filename = filename .. key .. "_" .. params[key] .. "_"
    end
    -- Trim the last underscore, prepend the exchange name, and append the file extension
    filename =  "/" .. exchange_name .. uri .. "/" .. filename:sub(1, -2) .. ".json"

    -- Pass the file name back to the caller
    return filename
end

return _M
