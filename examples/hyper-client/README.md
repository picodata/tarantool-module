# Example hyper HTTP client

This is a simple example of how you can use **tarantool** with a
**hyper** based async http client running on our built-in fiber based runtime.

Build the code:
```bash
cargo build -p hyper-client
```

Make sure `LUA_CPATH` environment variable is setup correctly, e.g. if your
target director is `./target`:
```bash
export LUA_CPATH='./target/debug/lib?.so'
```

Start tarantool with the provided entry script:
```bash
tarantool examples/hyper-client/src/init.lua
```

Send a `GET` request to any url:
```bash
tarantool> box.func['hyper_client.get']:call({"http://example.org"})
```
