/*
BSD 2-Clause License

Copyright (c) 2019, Tarantool
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
*/

package controllers

import (
	"context"
	"fmt"
	"strings"
	"time"

	appsv1 "k8s.io/api/apps/v1"
	corev1 "k8s.io/api/core/v1"
	"k8s.io/apimachinery/pkg/api/errors"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/runtime"
	"k8s.io/apimachinery/pkg/types"
	ctrl "sigs.k8s.io/controller-runtime"
	"sigs.k8s.io/controller-runtime/pkg/client"
	"sigs.k8s.io/controller-runtime/pkg/controller/controllerutil"
	"sigs.k8s.io/controller-runtime/pkg/handler"
	"sigs.k8s.io/controller-runtime/pkg/log"
	"sigs.k8s.io/controller-runtime/pkg/reconcile"
	"sigs.k8s.io/controller-runtime/pkg/source"

	"github.com/google/uuid"
	tarantooliov1alpha1 "github.com/tarantool/tarantool-operator/api/v1alpha1"
	"github.com/tarantool/tarantool-operator/controllers/tarantool"
	"github.com/tarantool/tarantool-operator/controllers/topology"
	"github.com/tarantool/tarantool-operator/controllers/utils"
)

var space = uuid.MustParse("73692FF6-EB42-46C2-92B6-65C45191368D")

// ClusterReconciler reconciles a Cluster object
type ClusterReconciler struct {
	client.Client
	Scheme *runtime.Scheme
}

// Checking for a leader in the cluster Endpoint annotation
func IsLeaderExists(ep *corev1.Endpoints) bool {
	leader, ok := ep.Annotations["tarantool.io/leader"]
	if !ok || leader == "" {
		return false
	}

	for _, addr := range ep.Subsets[0].Addresses {
		if leader == fmt.Sprintf("%s:%s", addr.IP, "8081") {
			return true
		}
	}
	return false
}

// HasInstanceUUID .
func HasInstanceUUID(o *corev1.Pod) bool {
	annotations := o.Labels
	if _, ok := annotations["tarantool.io/instance-uuid"]; ok {
		return true
	}

	return false
}

// SetInstanceUUID .
func SetInstanceUUID(o *corev1.Pod) *corev1.Pod {
	labels := o.Labels
	if len(o.GetName()) == 0 {
		return o
	}
	instanceUUID := uuid.NewSHA1(space, []byte(o.GetName()))
	labels["tarantool.io/instance-uuid"] = instanceUUID.String()

	o.SetLabels(labels)
	return o
}

//+kubebuilder:rbac:groups=tarantool.io,resources=clusters,verbs=get;list;watch;create;update;patch;delete
//+kubebuilder:rbac:groups=tarantool.io,resources=clusters/status,verbs=get;update;patch
//+kubebuilder:rbac:groups=tarantool.io,resources=clusters/finalizers,verbs=update
//+kubebuilder:rbac:groups=apps,resources=statefulsets,verbs=get;create;update;watch;list;patch;delete
//+kubebuilder:rbac:groups="",resources=pods,verbs=get;create;update;watch;list;patch;delete
//+kubebuilder:rbac:groups="",resources=services,verbs=get;create;update;watch;list;patch;delete
//+kubebuilder:rbac:groups="",resources=endpoints,verbs=get;create;update;watch;list;patch;delete

// Reconcile is part of the main kubernetes reconciliation loop which aims to
// move the current state of the cluster closer to the desired state.
// TODO(user): Modify the Reconcile function to compare the state specified by
// the Cluster object against the actual cluster state, and then
// perform operations to make the cluster state reflect the state specified by
// the user.
//
// For more details, check Reconcile and its Result here:
// - https://pkg.go.dev/sigs.k8s.io/controller-runtime@v0.10.0/pkg/reconcile
func (r *ClusterReconciler) Reconcile(ctx context.Context, req ctrl.Request) (ctrl.Result, error) {
	reqLogger := log.FromContext(ctx)
	reqLogger.Info("Reconciling Cluster")

	// do nothing if no Cluster
	cluster := &tarantooliov1alpha1.Cluster{}
	if err := r.Get(context.TODO(), req.NamespacedName, cluster); err != nil {
		if errors.IsNotFound(err) {
			return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, nil
		}

		return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
	}

	clusterSelector, err := metav1.LabelSelectorAsSelector(cluster.Spec.Selector)
	if err != nil {
		return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
	}

	roleList := &tarantooliov1alpha1.RoleList{}
	if err := r.List(context.TODO(), roleList, &client.ListOptions{LabelSelector: clusterSelector, Namespace: req.NamespacedName.Namespace}); err != nil {
		if errors.IsNotFound(err) {
			return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, nil
		}

		return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
	}

	for _, role := range roleList.Items {
		if metav1.IsControlledBy(&role, cluster) {
			reqLogger.Info("Already owned", "Role.Name", role.Name)
			continue
		}
		annotations := role.GetAnnotations()
		if annotations == nil {
			annotations = make(map[string]string)
		}
		annotations["tarantool.io/cluster-id"] = cluster.GetName()
		role.SetAnnotations(annotations)
		if err := controllerutil.SetControllerReference(cluster, &role, r.Scheme); err != nil {
			return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
		}
		if err := r.Update(context.TODO(), &role); err != nil {
			return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
		}

		reqLogger.Info("Set role ownership", "Role.Name", role.GetName(), "Cluster.Name", cluster.GetName())
	}

	reqLogger.Info("Roles reconciled, moving to pod reconcile")

	// ensure cluster wide Service exists
	svc := &corev1.Service{}
	if err := r.Get(context.TODO(), types.NamespacedName{Namespace: cluster.GetNamespace(), Name: cluster.GetName()}, svc); err != nil {
		if errors.IsNotFound(err) {
			svc.Name = cluster.GetName()
			svc.Namespace = cluster.GetNamespace()
			svc.Spec = corev1.ServiceSpec{
				Selector:  cluster.Spec.Selector.MatchLabels,
				ClusterIP: "None",
				Ports: []corev1.ServicePort{
					{
						Name:     "app",
						Port:     3301,
						Protocol: "TCP",
					},
				},
			}

			if err := controllerutil.SetControllerReference(cluster, svc, r.Scheme); err != nil {
				return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
			}

			if err := r.Create(context.TODO(), svc); err != nil {
				return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
			}
		}
	}

	// ensure Cluster leader elected
	ep := &corev1.Endpoints{}
	if err := r.Get(context.TODO(), types.NamespacedName{Namespace: cluster.GetNamespace(), Name: cluster.GetName()}, ep); err != nil {
		return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
	}
	if len(ep.Subsets) == 0 || len(ep.Subsets[0].Addresses) == 0 {
		reqLogger.Info("No available Endpoint resource configured for Cluster, waiting")
		return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, nil
	}

	if !IsLeaderExists(ep) {
		leader := fmt.Sprintf("%s:%s", ep.Subsets[0].Addresses[0].IP, "8081")

		if ep.Annotations == nil {
			ep.Annotations = make(map[string]string)
		}

		ep.Annotations["tarantool.io/leader"] = leader
		if err := r.Update(context.TODO(), ep); err != nil {
			return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
		}
	}

	stsList := &appsv1.StatefulSetList{}
	if err := r.List(context.TODO(), stsList, &client.ListOptions{LabelSelector: clusterSelector, Namespace: req.NamespacedName.Namespace}); err != nil {
		if errors.IsNotFound(err) {
			return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, nil
		}

		return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
	}

	topologyClient := topology.NewBuiltInTopologyService(
		topology.WithTopologyEndpoint(fmt.Sprintf("http://%s/admin/api", ep.Annotations["tarantool.io/leader"])),
		topology.WithClusterID(cluster.GetName()),
	)

	for _, sts := range stsList.Items {
		for i := 0; i < int(*sts.Spec.Replicas); i++ {
			pod := &corev1.Pod{}
			name := types.NamespacedName{
				Namespace: req.Namespace,
				Name:      fmt.Sprintf("%s-%d", sts.GetName(), i),
			}
			if err := r.Get(context.TODO(), name, pod); err != nil {
				if errors.IsNotFound(err) {
					return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, nil
				}

				return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
			}

			podLogger := reqLogger.WithValues("Pod.Name", pod.GetName())
			if HasInstanceUUID(pod) {
				continue
			}
			podLogger.Info("starting: set instance uuid")
			pod = SetInstanceUUID(pod)

			if err := r.Update(context.TODO(), pod); err != nil {
				return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
			}

			podLogger.Info("success: set instance uuid", "UUID", pod.GetLabels()["tarantool.io/instance-uuid"])
			return ctrl.Result{Requeue: true}, nil
		}

		for i := 0; i < int(*sts.Spec.Replicas); i++ {
			pod := &corev1.Pod{}
			name := types.NamespacedName{
				Namespace: req.Namespace,
				Name:      fmt.Sprintf("%s-%d", sts.GetName(), i),
			}
			if err := r.Get(context.TODO(), name, pod); err != nil {
				if errors.IsNotFound(err) {
					return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, nil
				}

				return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
			}

			if tarantool.IsJoined(pod) {
				continue
			}

			if err := topologyClient.Join(pod); err != nil {
				if topology.IsAlreadyJoined(err) {
					tarantool.MarkJoined(pod)
					if err := r.Update(context.TODO(), pod); err != nil {
						return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
					}
					reqLogger.Info("Already joined", "Pod.Name", pod.Name)
					continue
				}

				if topology.IsTopologyDown(err) {
					reqLogger.Info("Topology is down", "Pod.Name", pod.Name)
					continue
				}

				reqLogger.Error(err, "Join error")
				return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, nil
			} else {
				tarantool.MarkJoined(pod)
				if err := r.Update(context.TODO(), pod); err != nil {
					return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
				}
			}

			return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, nil
		}
	}

	for _, sts := range stsList.Items {
		stsAnnotations := sts.GetAnnotations()
		weight := stsAnnotations["tarantool.io/replicaset-weight"]

		if weight == "0" {
			reqLogger.Info("weight is set to 0, checking replicaset buckets for scheduled deletion")
			data, err := topologyClient.GetServerStat()
			if err != nil {
				reqLogger.Error(err, "failed to get server stats")
			} else {
				for i := 0; i < len(data.Stats); i++ {
					if strings.HasPrefix(data.Stats[i].URI, sts.GetName()) {
						reqLogger.Info("Found statefulset to check for buckets count", "sts.Name", sts.GetName())

						bucketsCount := data.Stats[i].Statistics.BucketsCount
						if bucketsCount == 0 {
							reqLogger.Info("replicaset has migrated all of its buckets away, schedule to remove", "sts.Name", sts.GetName())

							stsAnnotations["tarantool.io/scheduledDelete"] = "1"
							sts.SetAnnotations(stsAnnotations)
							if err := r.Update(context.TODO(), &sts); err != nil {
								reqLogger.Error(err, "failed to set scheduled deletion annotation")
							}
						} else {
							reqLogger.Info("replicaset still has buckets, retry checking on next run", "sts.Name", sts.GetName(), "buckets", bucketsCount)
						}
					}
				}
			}
		}

		for i := 0; i < int(*sts.Spec.Replicas); i++ {
			pod := &corev1.Pod{}
			name := types.NamespacedName{
				Namespace: req.Namespace,
				Name:      fmt.Sprintf("%s-%d", sts.GetName(), i),
			}

			if err := r.Get(context.TODO(), name, pod); err != nil {
				if errors.IsNotFound(err) {
					return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, nil
				}

				return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
			}

			if !tarantool.IsJoined(pod) {
				reqLogger.Info("Not all instances joined, skip weight change", "StatefulSet.Name", sts.GetName())
				return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, nil
			}
		}

		if err := topologyClient.SetWeight(sts.GetLabels()["tarantool.io/replicaset-uuid"], weight); err != nil {
			return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
		}
	}

	for _, sts := range stsList.Items {
		replicasetUUID := sts.GetLabels()["tarantool.io/replicaset-uuid"]

		actualRoles, err := topologyClient.GetReplicasetRolesFromService(replicasetUUID)
		if err != nil {
			reqLogger.Error(err, "Getting roles from server")
			return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
		}

		desireRoles, err := topology.GetRoles(&sts.ObjectMeta)
		if err != nil {
			reqLogger.Error(err, "Getting roles from statefulset")
			return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
		}

		if utils.IsRolesEquals(actualRoles, desireRoles) {
			continue
		}
		reqLogger.Info("Update replicaset roles", "id", replicasetUUID, "from", actualRoles, "to", desireRoles)

		err = topologyClient.SetReplicasetRoles(replicasetUUID, desireRoles)
		if err != nil {
			reqLogger.Error(err, "Setting new replicaset roles")
			return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
		}
	}

	for _, sts := range stsList.Items {
		stsAnnotations := sts.GetAnnotations()
		if stsAnnotations["tarantool.io/isBootstrapped"] != "1" {
			reqLogger.Info("cluster is not bootstrapped, bootstrapping", "Statefulset.Name", sts.GetName())
			if err := topologyClient.BootstrapVshard(); err != nil {
				if topology.IsAlreadyBootstrapped(err) {
					stsAnnotations["tarantool.io/isBootstrapped"] = "1"
					sts.SetAnnotations(stsAnnotations)

					if err := r.Update(context.TODO(), &sts); err != nil {
						reqLogger.Error(err, "failed to set bootstrapped annotation")
					}

					reqLogger.Info("Added bootstrapped annotation", "StatefulSet.Name", sts.GetName())

					cluster.Status.State = "Ready"
					err = r.Status().Update(context.TODO(), cluster)
					if err != nil {
						return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
					}
					return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, nil
				}

				reqLogger.Error(err, "Bootstrap vshard error")
				return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
			}
		} else {
			reqLogger.Info("cluster is already bootstrapped, not retrying", "Statefulset.Name", sts.GetName())
		}

		if stsAnnotations["tarantool.io/failoverEnabled"] == "1" {
			reqLogger.Info("failover is enabled, not retrying")
		} else {
			if err := topologyClient.SetFailover(true); err != nil {
				reqLogger.Error(err, "failed to enable cluster failover")
			} else {
				reqLogger.Info("enabled failover")

				stsAnnotations["tarantool.io/failoverEnabled"] = "1"
				sts.SetAnnotations(stsAnnotations)
				if err := r.Update(context.TODO(), &sts); err != nil {
					reqLogger.Error(err, "failed to set failover enabled annotation")
				}
			}
		}
	}

	return ctrl.Result{RequeueAfter: time.Duration(5 * time.Second)}, nil
}

// SetupWithManager sets up the controller with the Manager.
func (r *ClusterReconciler) SetupWithManager(mgr ctrl.Manager) error {
	return ctrl.NewControllerManagedBy(mgr).
		For(&tarantooliov1alpha1.Cluster{}).
		Watches(&source.Kind{Type: &tarantooliov1alpha1.Cluster{}}, &handler.EnqueueRequestForObject{}).
		Watches(&source.Kind{Type: &corev1.Pod{}}, handler.EnqueueRequestsFromMapFunc(func(a client.Object) []reconcile.Request {
			if a.GetLabels() == nil {
				return []ctrl.Request{}
			}
			return []ctrl.Request{
				{NamespacedName: types.NamespacedName{
					Namespace: a.GetNamespace(),
					Name:      a.GetLabels()["tarantool.io/cluster-id"],
				}},
			}
		})).
		Complete(r)
}
