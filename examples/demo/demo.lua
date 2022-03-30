#!/usr/bin tarantool

ffi = require 'ffi'
ffi.cdef [[ void setup(void); ]]
demo = ffi.load 'demo'
demo.setup()

min = 1
max = 6

box.func['demo.insert']:call { 'test_space', { {4, 'ass'}, {5, 'fuck'} } }
result = box.func['demo.example']:call { 'test_space', min, max }
print(require 'yaml'.encode(result))

sum = 0
values = {}
for _, row in box.space.test_space:pairs(min, { iterator = box.index.GE }) do
    if row.id > max then
        break
    end
    sum = sum + row.id
    table.insert(values, row.text)
end

print(require 'yaml'.encode{{ sum = sum, concat = table.concat(values) }})

assert(1 == 2)
os.exit(0)
