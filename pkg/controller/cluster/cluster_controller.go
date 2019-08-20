package cluster

import (
	"context"

	goerrors "errors"

	"github.com/google/uuid"
	tarantoolv1alpha1 "gitlab.com/tarantool/sandbox/tarantool-operator/pkg/apis/tarantool/v1alpha1"
	"gitlab.com/tarantool/sandbox/tarantool-operator/pkg/topology"
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

type ResponseError struct {
	Message string `json:"message"`
}
type JoinResponseData struct {
	JoinInstance bool `json:"join_instance"`
}
type JoinResponse struct {
	Errors []*ResponseError  `json:"errors,omitempty"`
	Data   *JoinResponseData `json:"data,omitempty"`
}

type ExpelResponseData struct {
	ExpelInstance bool `json:"expel_instance"`
}
type ExpelResponse struct {
	Errors []*ResponseError   `json:"errors,omitempty"`
	Data   *ExpelResponseData `json:"data,omitempty"`
}

func HasInstanceUUID(o *corev1.Pod) bool {
	annotations := o.Labels
	if _, ok := annotations["tarantool.io/instance-uuid"]; ok {
		return true
	}

	return false
}

func SetInstanceUUID(o *corev1.Pod) *corev1.Pod {
	labels := o.Labels
	if len(o.GetName()) == 0 {
		return o
	}
	instanceUUID, _ := uuid.NewUUID()
	if labels == nil {
		labels = make(map[string]string)
	}
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

	cluster := &tarantoolv1alpha1.Cluster{}
	err := r.client.Get(context.TODO(), request.NamespacedName, cluster)
	if err != nil {
		if errors.IsNotFound(err) {
			return reconcile.Result{}, nil
		}

		return reconcile.Result{}, err
	}

	clusterSelector, err := metav1.LabelSelectorAsSelector(cluster.Spec.Selector)
	if err != nil {
		return reconcile.Result{}, err
	}

	roleList := &tarantoolv1alpha1.RoleList{}
	if err := r.client.List(context.TODO(), &client.ListOptions{LabelSelector: clusterSelector}, roleList); err != nil {
		if errors.IsNotFound(err) {
			return reconcile.Result{}, nil
		}

		return reconcile.Result{}, err
	}

	for _, role := range roleList.Items {
		if metav1.IsControlledBy(&role, cluster) {
			reqLogger.Info("Already owned", "Role.Name", role.Name)
			continue
		}
		if err := controllerutil.SetControllerReference(cluster, &role, r.scheme); err != nil {
			return reconcile.Result{}, err
		}
		if err := r.client.Update(context.TODO(), &role); err != nil {
			return reconcile.Result{}, err
		}

		reqLogger.Info("Set role ownership", "Role.Name", role.GetName(), "Cluster.Name", cluster.GetName())
	}

	reqLogger.Info("Roles reconciled, moving to pod reconcile")

	podList := &corev1.PodList{}
	if err := r.client.List(context.TODO(), &client.ListOptions{LabelSelector: clusterSelector}, podList); err != nil {
		if errors.IsNotFound(err) {
			return reconcile.Result{}, nil
		}

		return reconcile.Result{}, err
	}

	for _, pod := range podList.Items {
		podLogger := reqLogger.WithValues("Pod.Name", pod.GetName())
		if HasInstanceUUID(&pod) {
			continue
		}
		podLogger.Info("starting: set instance uuid")
		pod = *SetInstanceUUID(&pod)

		if err := r.client.Update(context.TODO(), &pod); err != nil {
			return reconcile.Result{}, err
		}

		podLogger.Info("success: set instance uuid", "UUID", pod.GetLabels()["tarantool.io/instance-uuid"])
		return reconcile.Result{Requeue: true}, nil
	}

	topologyClient := topology.NewBuiltInTopologyService()
	for _, pod := range podList.Items {
		if err := topologyClient.Join(&pod); err != nil {
			if topology.IsAlreadyJoined(err) {
				reqLogger.Info("Already joined", "Pod.Name", pod.Name)
				continue
			}

			if topology.IsTopologyDown(err) {
				reqLogger.Info("Topology is down", "Pod.Name", pod.Name)
				continue
			}

			reqLogger.Info("unknown error")

			return reconcile.Result{}, err
		}

		return reconcile.Result{Requeue: true}, goerrors.New("Not all pods joined, requeue")
	}

	if err := topologyClient.BootstrapVshard(); err != nil {
		return reconcile.Result{}, err
	}

	return reconcile.Result{}, nil
}
