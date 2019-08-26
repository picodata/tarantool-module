# Key Value Storage

examples/kv is a Tarantool based distributed key value storage. Data accessed via HTTP REST API.

### Application topology overview

![App topology](./assets/topology.png)

### Running example

Assuming commands executed from repository root and Tarantool Operator is up and running.

1. Create cluster:

    ```shell
    kubectl create -f examples/kv/deployment.yaml
    ```

1. Wait until cluster Pod's are up:

    ```shell
    kubectl get pods --watch
    ```

1. Access cluster web ui:

    get minikube vm ip:

    ```shell
    minikube ip
    ```

    navigate to **http://MINIKUBE_IP** with your browser. Replace MINIKUBE_IP with above command output

1. Access KV API:

    store some value

    ```shell
    curl -XPOST http://MINIKUBE_IP/kv -d '{"key":"key_1", "value": "value_1"}'
    ```

1. Access stored values

    ```shell
    curl http://MINIKUBE_IP/kv_dump
    ```

### Scaling example app

1. Increase number of replicasets in Storages Role:

    ```shell
    kubectl scale roles.tarantool.io storage --replicas=3
    ```

    This will result in addition of one more replicaset to existing cluster.

    View cluster topology chnagin via cluster web ui.

1. Increase number of replicas across all Storages Role replicasets:

    ```shell
    kubectl edit replicasettemplates.tarantool.io storage-template
    ```

    This will open text editor. Change spec.replicas field value to 3 then save and exit editor.

    This will result in addition of one more replica to each replicaset consisting Storages Role.

    View cluster topology changing via cluster web ui

### Running tests

```shell
make build
make start
./bootstrap.sh
make test
```
