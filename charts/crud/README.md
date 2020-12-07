# CRUD base ready-to-use application

## Chart preparation

Build chart dependencies

```shell
$ helm repo add tarantool https://tarantool.github.io/tarantool-operator
$ helm dependency build ./charts/crud/
```

## Deploy

Run `helm install`:

```shell
$ helm install crud-app ./charts/crud/ --namespace tarantool --create-namespace
---
NAME: crud-app
LAST DEPLOYED: Wed Dec  2 15:58:11 2020
NAMESPACE: tarantool
STATUS: deployed
REVISION: 1
```

Check pods:

```shell
$ kubectl -n tarantool get pods
---
NAME                                  READY   STATUS    RESTARTS   AGE
crud-0-0                              1/1     Running   0          2m28s
crud-0-1                              1/1     Running   0          119s
crud-1-0                              1/1     Running   0          2m28s
crud-1-1                              1/1     Running   0          2m10s
tarantool-operator-644f487f87-4cqlv   1/1     Running   0          2m31s
```

## Schema creation

Go to Cartridge webUI

```shell
$ kubectl -n tarantool port-forward crud-0-0 8081:8081
```

Add new migration file `migrations/source/000_bootstrap.lua` on code page and apply configuration:

![](https://i.imgur.com/6PMZ5Ui.png)

```lua
return {
    up = function()
        local utils = require('migrator.utils')
        local s = box.schema.create_space('customer', {
            format = {
                { name = 'id', type = 'number' },
                { name = 'name', type = 'string' },
                { name = 'age', type = 'number' },
                { name = 'bucket_id', type = 'unsigned' },
            },
            if_not_exists = true,
        })
        s:create_index('primary', {
            parts = { 'key' },
            if_not_exists = true,
        })
        s:create_index('bucket_id', {
            parts = { 'bucket_id' },
            if_not_exists = true,
            unique = false
        })
        utils.register_sharding_key('customer', {'bucket_id'})
        return true
    end
}
```

And apply migration:
```shell
$ curl --header "Content-Type: application/json" \
       --request POST \
       --data '{}' \
       http://localhost:8081/migrations/up
---
{"applied":["000_bootstrap.lua"]}
```

## Using

Connect to any instance:

```shell
$ kubectl -n tarantool exec -it crud-0-0 -- /bin/bash
```
```shell
bash-4.4$ tarantoolctl connect admin:crud-app-cluster-cookie@localhost:3301
connected to localhost:3301
```

Try insert objects:

```shell
localhost:3301> crud.insert('customer', {0, "Ivan", 22, box.NULL})
---
- metadata: [{'name': 'id', 'type': 'number'}, {'name': 'name', 'type': 'string'},
    {'name': 'age', 'type': 'number'}, {'name': 'bucket_id', 'type': 'unsigned'}]
  rows:
  - [0, 'Ivan', 22, 18560]
...

localhost:3301> crud.insert('customer', {1, "Artem", 21, box.NULL})
---
- metadata: [{'name': 'id', 'type': 'number'}, {'name': 'name', 'type': 'string'},
    {'name': 'age', 'type': 'number'}, {'name': 'bucket_id', 'type': 'unsigned'}]
  rows:
  - [1, 'Artem', 21, 12477]
...

localhost:3301> crud.insert('customer', {2, "Denis", 20, box.NULL})
---
- metadata: [{'name': 'id', 'type': 'number'}, {'name': 'name', 'type': 'string'},
    {'name': 'age', 'type': 'number'}, {'name': 'bucket_id', 'type': 'unsigned'}]
  rows:
  - [2, 'Denis', 20, 21401]
...
```

And execute simple select:

```shell 
localhost:3301> crud.select('customer', {{'>=', 'age', 21}})
---
- metadata: [{'name': 'id', 'type': 'number'}, {'name': 'name', 'type': 'string'},
    {'name': 'age', 'type': 'number'}, {'name': 'bucket_id', 'type': 'unsigned'}]
  rows:
  - [0, 'Ivan', 22, 18560]
  - [1, 'Artem', 21, 12477]
...
```

## Customization

By default the cluster contains two replicasets of two nodes (master - replica). If you want to change this configuration, you must describe it in the file `crud_values.yaml`

For example:
```yaml
cartridge:
    RoleConfig:
    - RoleName: crud
      ReplicaCount: 2
      ReplicaSetCount: 5
      DiskSize: 1Gi
      CPUallocation: 0.25
      MemtxMemoryMB: 256
      RolesToAssign:
        - crud-storage
        - crud-router
        - migrator
        - metrics
```
And pass this values to `helm install`:

```shell
$ helm install crud-app -f crud_values.yaml ./charts/crud/ --namespace tarantool --create-namespace
```

**NOTE** - all specified fields are required. Look at [this](https://github.com/tarantool/tarantool-operator/issues/44) ticket.