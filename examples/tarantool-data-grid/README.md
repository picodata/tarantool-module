# Tarantool Data Grid

This folder contains example on how to run [Tarantool Data Grid](https://www.tarantool.io/en/datagrid/) 
on Kubernetes.

## Requirements

To run this example you will need: 

- [helm](https://helm.sh/docs/intro/install/)
- [kubectl](https://kubernetes.io/docs/tasks/tools/install-kubectl/)
- Tarantool Data Grid docker image

## Running example

1. Install Tarantool Helm repo:

    ```shell
    $ helm repo add tarantool https://tarantool.github.io/tarantool-operator
    ```

2. Install Tarantool Kubernetes Operator:

    ```shell
    $ helm install --set namespace=tarantool tarantool-operator tarantool/tarantool-operator --namespace tarantool --create-namespace --version 0.0.6
    ---
    NAME: tarantool-operator
    NAMESPACE: tarantool
    STATUS: deployed
    TEST SUITE: None
    ```

    wait for Operator to become Running:

    ```shell
    $ kubeclt -n tarantool get pods -w
    ---
    NAME                                 READY   STATUS    RESTARTS   AGE
    tarantool-operator-xxx-yyy           1/1     Running   0          3s
    ```

3. Get Tarantool Data Grid image:

    - go to [tarantool.io](https://tarantool.io)

    - authorize via "Sign In"

    - then go to "Customer zone" > "tdg"

    - download archive with docker image, this example tested on tdg-1.6.8-xxxxxxx.docker-image.tar.gz

    - import image from archive:
    
        ```shell
        $ docker image load -i tdg-1.6.8-xxxxxxx.docker-image.tar.gz
        ---
        Loaded image ID: sha256:b6206567xxxxxxxxxxxxxx
        ```

    - tag loaded image:
    
        ```shell
        $ docker tag b6206567xxxxxxxxxxxxxx tdg:1.6.8
        ```
    
    - make image available to k8s nodes:

        - push image to your registry, so k8s will be able to download it

        - manually upload image to each node



4. Take ```values.yaml``` from this folder, change `image.repository` and `image.tag` to point to tdg image and install it with helm:

    ```shell
    $ helm install -f values.yaml tdg-app tarantool/cartridge --namespace tarantool --version 0.0.6
    ---
    NAME: tdg-app
    NAMESPACE: tarantool
    STATUS: deployed
    TEST SUITE: None
    ```

Wait until pods up and cluster is ready. 

5. Access TDG web ui:

    ```shell
    kubectl -n tarantool port-forward service/routers 8081:8081
    ---
    Forwarding from 127.0.0.1:8081 -> 8081
    Forwarding from [::1]:8081 -> 8081
    ...
    ```

    Now you should be able to navigate to 127.0.0.1:8081 with your browser and access tdg web ui.

Now you should have running Tarantool Data Grid cluster. 

Proceed to our [TDG examples](https://github.com/tarantool/examples/tree/master/tdg) repository to start using cluster.