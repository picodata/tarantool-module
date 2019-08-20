package = 'key-value'
version = 'scm-1'

source  = {
    url = '/dev/null',
}

dependencies = {
    'tarantool',
    'lua >= 5.1',
    'checks == 2.1.1-1',
    'cluster == 0.9.2-1',
}

build = {
    type = 'none';
}
