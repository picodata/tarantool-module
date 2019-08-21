# Tarantool Kubernetes Operator

### Как запустить оператор и exapmles/kv приложение на minikube:

1. Установить kubectl и minikube

    - [kubectl](https://kubernetes.io/docs/tasks/tools/install-kubectl/#install-with-homebrew-on-macos)

    - [minikube](https://kubernetes.io/docs/tasks/tools/install-minikube/)


    ```shell
    brew install kubernetes-cli
    brew cask install minikube
    ```

1. Создать кластер k8s на minikube

    ```shell
    minikube start
    ```

    Проверить, что все запустилось:

    ```shell
    minikube status
    ```

    если все хорошо то увидим нечто подобное

    ```shell
    host: Running
    kubelet: Running
    apiserver: Running
    ```

1. Авторизоваться в registry.gitlab.com:

    ```shell
    docker login registry.gitlab.com -u YOUR_GITLAB_USER_NAME -p YOUR_GITLAB_USER_PASSWORD
    ```

    где заменить

    **YOUR_GITLAB_USER_NAME** на имя пользователя на gitlab.com

    **YOUR_GITLAB_USER_PASSWORD** на пароль пользователя на gitlab.com

1. Создать secret в k8s для скачивания docker image с gitlab.com:

    ```shell
    kubectl create secret generic gl-regcred --from-file=.dockerconfigjson=$HOME/.docker/config.json --type=kubernetes.io/dockerconfigjson
    ```

1. Создать необходимые оператору для работы ресурсы:

    ```shell
    kubectl create -f deploy/service_account.yaml
    kubectl create -f deploy/role.yaml
    kubectl create -f deploy/role_binding.yaml
    ```

1. Создать CRD 

    ```shell
    kubectl create -f deploy/crds/tarantool_v1alpha1_cluster_crd.yaml
    kubectl create -f deploy/crds/tarantool_v1alpha1_role_crd.yaml
    kubectl create -f deploy/crds/tarantool_v1alpha1_replicasettemplate_crd.yaml
    ```

1. Запустить оператор 

    ```shell
    kubectl create -f deploy/operator.yaml
    ```

1. Запустить examples/kv приложение

    ```shell
    kubectl create -f deploy/crds/tarantool_v1alpha1_tarantoolcluster_cr.yaml
    ```

1. Пробросить порты, чтобы можно было в админку смотреть

    ```shell
    kubectl port-forward service/topology 8081:8081
    ```

    эту команду может потребоваться выполнить несколько раз, пока не отработает

1. Зайти браузером на [http://127.0.0.1:8081](http://127.0.0.1:8081)
