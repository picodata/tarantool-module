
kubectl create -f deploy/service_account.yaml
kubectl create -f deploy/role.yaml
kubectl create -f deploy/role_binding.yaml

kubectl create -f deploy/crds/tarantool_v1alpha1_cluster_crd.yaml
kubectl create -f deploy/crds/tarantool_v1alpha1_role_crd.yaml
kubectl create -f deploy/crds/tarantool_v1alpha1_replicasettemplate_crd.yaml

kubectl create -f deploy/operator.yaml
