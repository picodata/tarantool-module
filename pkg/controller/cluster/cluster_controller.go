package cluster

import (
	"context"
	"fmt"
	"strings"
	"time"

	"github.com/google/uuid"
	tarantoolv1alpha1 "github.com/tarantool/tarantool-operator/pkg/apis/tarantool/v1alpha1"
	"github.com/tarantool/tarantool-operator/pkg/tarantool"
	"github.com/tarantool/tarantool-operator/pkg/topology"
	appsv1 "k8s.io/api/apps/v1"
	corev1 "k8s.io/api/core/v1"
	"k8s.io/apimachinery/pkg/api/errors"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/runtime"
	"k8s.io/apimachinery/pkg/types"
	"sigs.k8s.io/controller-runtime/pkg/client"
	"sigs.k8s.io/controller-runtime/pkg/controller"
	"sigs.k8s.io/controller-runtime/pkg/controller/controllerutil"
	"sigs.k8s.io/controller-runtime/pkg/handler"
	"sigs.k8s.io/controller-runtime/pkg/manager"
	"sigs.k8s.io/controller-runtime/pkg/reconcile"
	logf "sigs.k8s.io/controller-runtime/pkg/runtime/log"
	"sigs.k8s.io/controller-runtime/pkg/source"
)

var log = logf.Log.WithName("controller_cluster")
var space = uuid.MustParse("73692FF6-EB42-46C2-92B6-65C45191368D")

// ResponseError .
type ResponseError struct {
	Message string `json:"message"`
}

// JoinResponseData .
type JoinResponseData struct {
	JoinInstance bool `json:"join_instance"`
}

// JoinResponse .
type JoinResponse struct {
	Errors []*ResponseError  `json:"errors,omitempty"`
	Data   *JoinResponseData `json:"data,omitempty"`
}

// ExpelResponseData .
type ExpelResponseData struct {
	ExpelInstance bool `json:"expel_instance"`
}

// ExpelResponse .
type ExpelResponse struct {
	Errors []*ResponseError   `json:"errors,omitempty"`
	Data   *ExpelResponseData `json:"data,omitempty"`
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

// Add creates a new Cluster Controller and adds it to the Manager. The Manager will set fields on the Controller
// and Start it when the Manager is Started.
func Add(mgr manager.Manager) error {
	return add(mgr, newReconciler(mgr))
}

// newReconciler returns a new reconcile.Reconciler
func newReconciler(mgr manager.Manager) reconcile.Reconciler {
	return &ReconcileCluster{client: mgr.GetClient(), scheme: mgr.GetScheme()}
}

// add adds a new Controller to mgr with r as the reconcile.Reconciler
func add(mgr manager.Manager, r reconcile.Reconciler) error {
	// Create a new controller
	c, err := controller.New("cluster-controller", mgr, controller.Options{Reconciler: r})
	if err != nil {
		return err
	}

	// Watch for changes to primary resource Cluster
	err = c.Watch(&source.Kind{Type: &tarantoolv1alpha1.Cluster{}}, &handler.EnqueueRequestForObject{})
	if err != nil {
		return err
	}

	// TODO(user): Modify this to be the types you create that are owned by the primary resource
	// Watch for changes to secondary resource Pods and requeue the owner Cluster
	err = c.Watch(&source.Kind{Type: &corev1.Pod{}}, &handler.EnqueueRequestsFromMapFunc{
		ToRequests: handler.ToRequestsFunc(func(a handler.MapObject) []reconcile.Request {
			if a.Meta.GetLabels() == nil {
				return []reconcile.Request{}
			}
			return []reconcile.Request{
				{NamespacedName: types.NamespacedName{
					Namespace: a.Meta.GetNamespace(),
					Name:      a.Meta.GetLabels()["tarantool.io/cluster-id"],
				}},
			}
		}),
	})
	if err != nil {
		return err
	}

	return nil
}

// blank assignment to verify that ReconcileCluster implements reconcile.Reconciler
var _ reconcile.Reconciler = &ReconcileCluster{}

// ReconcileCluster reconciles a Cluster object
type ReconcileCluster struct {
	// This client, initialized using mgr.Client() above, is a split client
	// that reads objects from the cache and writes to the apiserver
	client client.Client
	scheme *runtime.Scheme
}

// Reconcile reads that state of the cluster for a Cluster object and makes changes based on the state read
// and what is in the Cluster.Spec
// TODO(user): Modify this Reconcile function to implement your Controller logic.  This example creates
// a Pod as an example
// Note:
// The Controller will requeue the Request to be processed again if the returned error is non-nil or
// Result.Requeue is true, otherwise upon completion it will remove the work from the queue.
func (r *ReconcileCluster) Reconcile(request reconcile.Request) (reconcile.Result, error) {
	reqLogger := log.WithValues("Request.Namespace", request.Namespace, "Request.Name", request.Name)
	reqLogger.Info("Reconciling Cluster")

	// do nothing if no Cluster
	cluster := &tarantoolv1alpha1.Cluster{}
	if err := r.client.Get(context.TODO(), request.NamespacedName, cluster); err != nil {
		if errors.IsNotFound(err) {
			return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, nil
		}

		return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
	}

	clusterSelector, err := metav1.LabelSelectorAsSelector(cluster.Spec.Selector)
	if err != nil {
		return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
	}

	roleList := &tarantoolv1alpha1.RoleList{}
	if err := r.client.List(context.TODO(), &client.ListOptions{LabelSelector: clusterSelector}, roleList); err != nil {
		if errors.IsNotFound(err) {
			return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, nil
		}

		return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
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
		if err := controllerutil.SetControllerReference(cluster, &role, r.scheme); err != nil {
			return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
		}
		if err := r.client.Update(context.TODO(), &role); err != nil {
			return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
		}

		reqLogger.Info("Set role ownership", "Role.Name", role.GetName(), "Cluster.Name", cluster.GetName())
	}

	reqLogger.Info("Roles reconciled, moving to pod reconcile")

	// ensure cluster wide Service exists
	svc := &corev1.Service{}
	if err := r.client.Get(context.TODO(), types.NamespacedName{Namespace: cluster.GetNamespace(), Name: cluster.GetName()}, svc); err != nil {
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

			if err := controllerutil.SetControllerReference(cluster, svc, r.scheme); err != nil {
				return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
			}

			if err := r.client.Create(context.TODO(), svc); err != nil {
				return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
			}
		}
	}

	// ensure Cluster leader elected
	ep := &corev1.Endpoints{}
	if err := r.client.Get(context.TODO(), types.NamespacedName{Namespace: cluster.GetNamespace(), Name: cluster.GetName()}, ep); err != nil {
		return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
	}
	if len(ep.Subsets) == 0 || len(ep.Subsets[0].Addresses) == 0 {
		reqLogger.Info("No available Endpoint resource configured for Cluster, waiting")
		return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, nil
	}

	leader, ok := ep.Annotations["tarantool.io/leader"]
	if !ok {
		if leader == "" {
			reqLogger.Info("leader is not elected")
			// return reconcile.Result{RequeueAfter: time.Duration(5000 * time.Millisecond)}, nil
		}

		leader = fmt.Sprintf("%s:%s", ep.Subsets[0].Addresses[0].IP, "8081")

		if ep.Annotations == nil {
			ep.Annotations = make(map[string]string)
		}

		ep.Annotations["tarantool.io/leader"] = leader
		if err := r.client.Update(context.TODO(), ep); err != nil {
			return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
		}
	}

	stsList := &appsv1.StatefulSetList{}
	if err := r.client.List(context.TODO(), &client.ListOptions{LabelSelector: clusterSelector}, stsList); err != nil {
		if errors.IsNotFound(err) {
			return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, nil
		}

		return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
	}

	topologyClient := topology.NewBuiltInTopologyService(topology.WithTopologyEndpoint(fmt.Sprintf("http://%s/admin/api", leader)), topology.WithClusterID(cluster.GetName()))
	for _, sts := range stsList.Items {
		for i := 0; i < int(*sts.Spec.Replicas); i++ {
			pod := &corev1.Pod{}
			name := types.NamespacedName{
				Namespace: request.Namespace,
				Name:      fmt.Sprintf("%s-%d", sts.GetName(), i),
			}
			if err := r.client.Get(context.TODO(), name, pod); err != nil {
				if errors.IsNotFound(err) {
					return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, nil
				}

				return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
			}

			podLogger := reqLogger.WithValues("Pod.Name", pod.GetName())
			if HasInstanceUUID(pod) {
				continue
			}
			podLogger.Info("starting: set instance uuid")
			pod = SetInstanceUUID(pod)

			if err := r.client.Update(context.TODO(), pod); err != nil {
				return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
			}

			podLogger.Info("success: set instance uuid", "UUID", pod.GetLabels()["tarantool.io/instance-uuid"])
			return reconcile.Result{Requeue: true}, nil
		}

		for i := 0; i < int(*sts.Spec.Replicas); i++ {
			pod := &corev1.Pod{}
			name := types.NamespacedName{
				Namespace: request.Namespace,
				Name:      fmt.Sprintf("%s-%d", sts.GetName(), i),
			}
			if err := r.client.Get(context.TODO(), name, pod); err != nil {
				if errors.IsNotFound(err) {
					return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, nil
				}

				return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
			}

			if tarantool.IsJoined(pod) {
				continue
			}

			if err := topologyClient.Join(pod); err != nil {
				if topology.IsAlreadyJoined(err) {
					tarantool.MarkJoined(pod)
					if err := r.client.Update(context.TODO(), pod); err != nil {
						return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
					}
					reqLogger.Info("Already joined", "Pod.Name", pod.Name)
					continue
				}

				if topology.IsTopologyDown(err) {
					reqLogger.Info("Topology is down", "Pod.Name", pod.Name)
					continue
				}

				reqLogger.Error(err, "Join error")
				return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, nil
			} else {
				tarantool.MarkJoined(pod)
				if err := r.client.Update(context.TODO(), pod); err != nil {
					return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
				}
			}

			return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, nil
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
							if err := r.client.Update(context.TODO(), &sts); err != nil {
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
				Namespace: request.Namespace,
				Name:      fmt.Sprintf("%s-%d", sts.GetName(), i),
			}

			if err := r.client.Get(context.TODO(), name, pod); err != nil {
				if errors.IsNotFound(err) {
					return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, nil
				}

				return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
			}

			if !tarantool.IsJoined(pod) {
				reqLogger.Info("Not all instances joined, skip weight change", "StatefulSet.Name", sts.GetName())
				return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, nil
			}
		}

		if err := topologyClient.SetWeight(sts.GetLabels()["tarantool.io/replicaset-uuid"], weight); err != nil {
			return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
		}

		// if stsAnnotations == nil {
		// 	stsAnnotations = make(map[string]string)
		// }

		// stsAnnotations["tarantool.io/replicaset-weight"] = "1"
		// sts.SetAnnotations(stsAnnotations)
		// if err := r.client.Update(context.TODO(), &sts); err != nil {
		// 	return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
		// }
	}

	for _, sts := range stsList.Items {
		stsAnnotations := sts.GetAnnotations()
		if stsAnnotations["tarantool.io/isBootstrapped"] != "1" {
			reqLogger.Info("cluster is not bootstrapped, bootstrapping", "Statefulset.Name", sts.GetName())
			if err := topologyClient.BootstrapVshard(); err != nil {
				if topology.IsAlreadyBootstrapped(err) {
					stsAnnotations["tarantool.io/isBootstrapped"] = "1"
					sts.SetAnnotations(stsAnnotations)

					if err := r.client.Update(context.TODO(), &sts); err != nil {
						reqLogger.Error(err, "failed to set bootstrapped annotation")
					}

					reqLogger.Info("Added bootstrapped annotation", "StatefulSet.Name", sts.GetName())

					cluster.Status.State = "Ready"
					err = r.client.Status().Update(context.TODO(), cluster)
					if err != nil {
						return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
					}
					return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, nil
				}

				reqLogger.Error(err, "Bootstrap vshard error")
				return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, err
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
				if err := r.client.Update(context.TODO(), &sts); err != nil {
					reqLogger.Error(err, "failed to set failover enabled annotation")
				}
			}
		}
	}

	return reconcile.Result{RequeueAfter: time.Duration(5 * time.Second)}, nil
}
