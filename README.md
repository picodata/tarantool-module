<a href="http://tarantool.org">
   <img src="https://avatars2.githubusercontent.com/u/2344919?v=2&s=250"
align="right">
</a>

# Tarantool Kubernetes operator

[![Test][gh-actions-badge]][gh-actions-url]

The Tarantool Operator provides automation that simplifies the administration
of [Tarantool Cartridge](https://github.com/tarantool/cartridge)-based cluster on Kubernetes.

The Operator introduces new API version `tarantool.io/v1alpha1` and installs
custom resources for objects of three custom types: Cluster, Role, and
ReplicasetTemplate.

## Table of contents

* [Resources](#resources)
* [Resource ownership](#resource-ownership)
* [Documentation](#documentation)
* [Deploying the Tarantool operator on minikube](#deploying-the-tarantool-operator-on-minikube)
* [Example: key-value storage](#example-application-key-value-storage)
  * [Application topology](#application-topology)
  * [Running the application](#running-the-application)
  * [Scaling the application](#scaling-the-application)
  * [Running tests](#running-tests)

## Resources

**Cluster** represents a single Tarantool Cartridge cluster.

**Role** represents a Tarantool Cartridge user role.

**ReplicasetTemplate** is a template for StatefulSets created as members of Role.

## Resource ownership

Resources managed by the Operator being deployed have the following resource
ownership hierarchy:

![Resource ownership](./assets/resource_map.png)

Resource ownership directly affects how Kubernetes garbage collector works.
If you execute a delete command on a parent resource, then all its dependants
will be removed.

## Documentation

The documentation is on the Tarantool official [website](https://www.tarantool.io/ru/doc/latest/book/cartridge/cartridge_kubernetes_guide/).


## Deploying the Tarantool operator on minikube

1. Install the required deployment utilities:

    * [kubectl](https://kubernetes.io/docs/tasks/tools/install-kubectl)
    * [helm](https://helm.sh/docs/intro/install/)

    Pick one of these to run a local kubernetes cluster
    * [minikube](https://kubernetes.io/docs/tasks/tools/install-minikube/)
    * [Windows Docker Desktop](https://docs.docker.com/docker-for-windows/#kubernetes)
    * [OSX Docker Desktop](https://docs.docker.com/docker-for-mac/#kubernetes)

    To install and configure a local minikube installation:

    Create a `minikube` cluster:

    ```shell
    $ minikube start --memory=4096
    ```

    You will need 4Gb of RAM allocated to the `minikube` cluster to run examples.

    Ensure `minikube` is up and running:

    ```shell
    $ minikube status
    ---
    minikube
    type: Control Plane
    host: Running
    kubelet: Running
    apiserver: Running
    kubeconfig: Configured
    ```

2. Build the operator image

    ```shell
    $ make docker-build
    ```
    
    By default, the image is tagged as `tarantool-operator:<VERSION>`

3. Add image to local minikube registry

    ```shell
    $ make push-to-minikube
    ---
    minikube image load tarantool-operator:0.0.9
    ```

> **NOTE**: If you want to use the [official docker image](https://hub.docker.com/r/tarantool/tarantool-operator/tags) of the Tarantool operator use the **helm charts from the tarantool helm repository**.
> Read more about this in the [documentation](https://www.tarantool.io/ru/doc/latest/book/cartridge/cartridge_kubernetes_guide/#launch-the-application).

4. Install the operator

    ```shell
    $ helm install -n tarantool-operator operator helm-charts/tarantool-operator \
                 --create-namespace \
                 --set image.repository=tarantool-operator \
                 --set image.tag=0.0.9
    ---
    NAME: operator
    LAST DEPLOYED: Wed Dec 15 22:54:13 2021
    NAMESPACE: tarantool-operator
    STATUS: deployed
    REVISION: 1
    TEST SUITE: None
    ```
    
    Or you can use make: 
    
    ```shell
    $ make helm-install-operator
    ```

    Ensure the operator is up:

    ```shell
    $ kubectl get pods -n tarantool-operator
    ---
    NAME                                  READY   STATUS    RESTARTS   AGE
    controller-manager-778db958cf-bhw6z   1/1     Running   0          77s
    ```

    Wait for `controller-manager-xxxxxx-xx` Pod's status to become `Running`.

## Example Application: key-value storage

`examples/kv` contains a Tarantool-based distributed key-value storage.
Data are accessed via HTTP REST API.

### Application topology

![App topology](./examples/kv/assets/topology.png)

### Running the application

We assume that commands are executed from the repository root and
Tarantool Operator is up and running.

1. Create a cluster:

    ```shell
    $ helm install -n tarantool-app cartridge-app helm-charts/tarantool-cartridge \
                 --create-namespace \
                 --set LuaMemoryReserveMB=256
    ---
    NAME: cartridge-app
    LAST DEPLOYED: Wed Dec 15 23:50:09 2021
    NAMESPACE: tarantool-app
    STATUS: deployed
    REVISION: 1
    ```

    Or you can use make: 
    
    ```shell
    $ make helm-install-cartridge-app
    ```

    Wait until all the cluster Pods are up (status becomes `Running`):

    ```shell
    $ kubectl -n cartridge-app get pods
    ---
    NAME          READY   STATUS    RESTARTS   AGE
    routers-0-0   1/1     Running   0          6m12s
    storage-0-0   1/1     Running   0          6m12s
    storage-0-1   1/1     Running   0          6m12s
    ```

2. Ensure cluster became operational:

    ```shell
    $ kubectl -n tarantool-app describe clusters.tarantool.io/tarantool-cluster
    ```

    wait until Status.State is Ready:

    ```shell
    ...
    Status:
      State:  Ready
    ...
    ```

3. Access the cluster web UI:

    ```shell
    $ kubectl -n cartridge-app port-forward routers-0-0 8081:8081
    ---
    Forwarding from 127.0.0.1:8081 -> 8081
    Forwarding from [::1]:8081 -> 8081
    Handling connection for 8081
    ````

4. Access the key-value API:

   1. Store some value:

       ```shell
       $ curl -XPOST http://localhost:8081/kv -d '{"key":"key_1", "value": "value_1"}'
       ---
       {"info":"Successfully created"}
       ```

   2. Access stored values:

       ```shell
       $ curl http://localhost:8081/kv_dump
       ---
       {"store":[{"key":"key_1","value":"value_1"}]}
       ```

### Scaling the application

Increase the number of replica sets in Storages Role:

In the cartridge helm chart, edit the `helm-charts/tarantool-cartridge/values.yaml` file to be

```yaml
- RoleName: storage
  ReplicaCount: 2
  ReplicaSetCount: 2
```

Then run:

```shell
$ helm upgrade -n tarantool-app cartridge-app helm-charts/tarantool-cartridge \
           --set LuaMemoryReserveMB=256
```

This will add another storage role replica set to the existing cluster. View the new cluster topology via the cluster web UI.

Read more about cluster management in the [documentation](https://www.tarantool.io/ru/doc/latest/book/cartridge/cartridge_kubernetes_guide/#cluster-management).

## Development

Use `make help` to describe all targets.

Below are some of them.

### Regenerate the Custom Resource Definitions

```shell
$ make manifests
```

### Building tarantool-operator docker image

```shell
$ make docker-build
```

### Running tests

```shell
$ make test
```

[gh-actions-badge]: https://github.com/tarantool/tarantool-operator/workflows/Test/badge.svg
[gh-actions-url]: https://github.com/tarantool/tarantool-operator/actions
