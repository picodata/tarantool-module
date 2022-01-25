package = 'kv'
version = 'scm-1'
source  = {
    url = '/dev/null',
}
-- Put any modules your app depends on here
dependencies = {
    'tarantool',
    'lua >= 5.1',
    'checks == 3.1.0-1',
    'cartridge == 2.6.0-1',
    'metrics == 0.9.0-1',
    'cartridge-cli-extensions == 1.1.1-1',
}
build = {
    type = 'none';
}
