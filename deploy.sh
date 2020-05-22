
if [ -z "$1" ]
  then
    echo "pass namespace as first argument e.g:"
    echo "    ./deploy.sh userscripting"
    exit 1
fi

kubectl -n $1 create -f deploy/service_account.yaml
kubectl -n $1 create -f deploy/role.yaml
kubectl -n $1 create -f deploy/role_binding.yaml

kubectl -n $1 create -f deploy/crds/tarantool_v1alpha1_cluster_crd.yaml
kubectl -n $1 create -f deploy/crds/tarantool_v1alpha1_role_crd.yaml
kubectl -n $1 create -f deploy/crds/tarantool_v1alpha1_replicasettemplate_crd.yaml

kubectl -n $1 create -f deployment.yaml
