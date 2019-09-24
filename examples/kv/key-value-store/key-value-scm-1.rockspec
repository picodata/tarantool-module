package = 'key-value'
version = 'scm-1'

source  = {
    url = '/dev/null',
}

dependencies = {
    'tarantool',
    'lua >= 5.1',
    'checks == 3.0.1-1',
    'cartridge == 1.0.0-1',
}

build = {
    type = 'none';
}
