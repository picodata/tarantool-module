# Key Value Storage

examples/kv is a Tarantool based distributed key value storage. Data accessed via HTTP REST API.

### Running example

Assuming commands executed from repository root and Tarantool Operator is up and running.

1. Create cluster:

    ```shell
    kubectl create -f deploy/crds/tarantool_v1alpha1_tarantoolcluster_cr.yaml
    ```

2. Wait until cluster Pod's are up:

    ```shell
    kubectl get pods --watch
    ```


