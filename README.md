# Tarantool Kubernetes Operator

### Как запустить оператор и exapmles/kv приложение на minikube

1. Установить kubectl и minikube

    - [kubectl](https://kubernetes.io/docs/tasks/tools/install-kubectl)

    - [minikube](https://kubernetes.io/docs/tasks/tools/install-minikube/)


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

    дождаться запуска pod'a с оператором

    ```shell
    kubectl get pods --watch
    ```

    ждем пока STATUS pod'a tarantool-operator-xxxxxx-xx не перейдет в Runnning

1. Запустить examples/kv приложение

    ```shell
    kubectl create -f deploy/crds/tarantool_v1alpha1_tarantoolcluster_cr.yaml
    ```

    ждем пока все pod'ы перейдут в STATUS=Running

    ```shell
    kubectl get pods --watch
    ```

1. Пробросить порты, чтобы можно было в админку смотреть

    ```shell
    kubectl port-forward service/topology 8081:8081
    ```

    эту команду может потребоваться выполнить несколько раз, пока не отработает

1. Зайти браузером на [http://127.0.0.1:8081](http://127.0.0.1:8081)
