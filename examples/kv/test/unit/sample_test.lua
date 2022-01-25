local t = require('luatest')
local g = t.group('unit_sample')

-- create your space here
g.before_all(function() end)

-- drop your space here
g.after_all(function() end)

g.test_sample = function()
    t.assert_equals(type(box.cfg), 'table')
end
