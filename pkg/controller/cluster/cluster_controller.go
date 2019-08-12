package cluster

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"strings"

	goerrors "errors"

	"github.com/google/uuid"
	tarantoolv1alpha1 "gitlab.com/tarantool/sandbox/tarantool-operator/pkg/apis/tarantool/v1alpha1"
	tntutils "gitlab.com/tarantool/sandbox/tarantool-operator/pkg/tarantool/utils"
	corev1 "k8s.io/api/core/v1"
	"k8s.io/apimachinery/pkg/api/errors"
	"k8s.io/apimachinery/pkg/labels"
	"k8s.io/apimachinery/pkg/runtime"
	"k8s.io/apimachinery/pkg/selection"
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

func HasInstanceUUID(o *corev1.Pod) bool {
	annotations := o.Labels
	if _, ok := annotations["tarantool.io/instance-uuid"]; ok {
		return true
	}

	return false
}

func SetInstanceUUID(o *corev1.Pod) *corev1.Pod {
	oldAnnotations := o.Labels
	instanceUUID, _ := uuid.NewUUID()
	if oldAnnotations == nil {
		oldAnnotations = make(map[string]string)
	}
	oldAnnotations["tarantool.io/instance-uuid"] = instanceUUID.String()

	o.SetLabels(oldAnnotations)
	return o
}

/**
* USER ACTION REQUIRED: This is a scaffold file intended for the user to modify with their own Controller
* business logic.  Delete these comments after modifying this file.*
 */

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
			if val, ok := a.Meta.GetLabels()["tarantool.io/replicaset-uuid"]; !ok || len(val) == 0 {
				return []reconcile.Request{}
			}

			return []reconcile.Request{
				{NamespacedName: types.NamespacedName{
					Namespace: a.Meta.GetNamespace(),
					Name:      a.Meta.GetLabels()["app.kubernetes.io/name"],
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

	// Fetch the Cluster instance
	cluster := &tarantoolv1alpha1.Cluster{}
	err := r.client.Get(context.TODO(), request.NamespacedName, cluster)
	if err != nil {
		if errors.IsNotFound(err) {
			return reconcile.Result{}, nil
		}

		return reconcile.Result{}, err
	}

	for _, role := range cluster.Spec.Roles {
		roleInstance := &tarantoolv1alpha1.Role{}
		err := r.client.Get(context.TODO(), types.NamespacedName{Namespace: request.Namespace, Name: role.GetObjectMeta().GetName()}, roleInstance)
		if err != nil {
			if errors.IsNotFound(err) {
				if err := tntutils.SetTarantoolClusterID(&role, cluster.GetName()); err != nil {
					return reconcile.Result{}, err
				}
				if err := tntutils.SetPartOf(&role, cluster.GetName()); err != nil {
					return reconcile.Result{}, err
				}
				if err := tntutils.SetComponent(&role, role.GetName()); err != nil {
					return reconcile.Result{}, err
				}
				if err := controllerutil.SetControllerReference(cluster, &role, r.scheme); err != nil {
					return reconcile.Result{}, err
				}
				if err := r.client.Create(context.TODO(), &role); err != nil {
					return reconcile.Result{}, err
				}
			}

			return reconcile.Result{}, err
		}
	}

	list := corev1.PodList{}
	selector := labels.NewSelector()
	requirement, err := labels.NewRequirement("app.kubernetes.io/name", selection.Equals, []string{cluster.Name})
	if err != nil {
		return reconcile.Result{}, err
	}
	selector = selector.Add(*requirement)
	err = r.client.List(context.TODO(), &client.ListOptions{LabelSelector: selector}, &list)
	if err != nil {
		if errors.IsNotFound(err) {
			return reconcile.Result{}, err
		}
	}

	if len(list.Items) == 0 {
		return reconcile.Result{}, goerrors.New("no pods to reconcile")
	}

	for _, pod := range list.Items {
		if !HasInstanceUUID(&pod) {
			pod = *SetInstanceUUID(&pod)

			if err := r.client.Update(context.TODO(), &pod); err != nil {
				return reconcile.Result{}, err
			}

			return reconcile.Result{Requeue: true}, nil
		}
		reqLogger.Info("reconcile Pod", "podName", pod.Name)

		podIP := pod.Status.PodIP
		if len(podIP) == 0 {
			return reconcile.Result{}, goerrors.New("Waiting for pod")
		}
		advURI := fmt.Sprintf("%s:3301", podIP)

		replicasetUUID, ok := pod.GetLabels()["tarantool.io/replicaset-uuid"]
		if !ok {
			return reconcile.Result{}, goerrors.New("replicaset uuid empty")
		}
		instanceUUID, ok := pod.GetLabels()["tarantool.io/instance-uuid"]
		if !ok {
			return reconcile.Result{}, goerrors.New("instance uuid empty")
		}
		role, ok := pod.GetLabels()["app.kubernetes.io/component"]
		if !ok {
			return reconcile.Result{}, goerrors.New("role undefined")
		}
		req := fmt.Sprintf("mutation {join_instance: join_server(uri: \\\"%s\\\",instance_uuid: \\\"%s\\\",replicaset_uuid: \\\"%s\\\",roles: [\\\"%s\\\"],timeout: 10)}", advURI, instanceUUID, replicasetUUID, role)

		j := fmt.Sprintf("{\"query\": \"%s\"}", req)

		rawResp, err := http.Post("http://127.0.0.1:8081/admin/api", "application/json", strings.NewReader(j))
		if err != nil {
			return reconcile.Result{}, err
		}
		defer rawResp.Body.Close()

		resp := &JoinResponse{Errors: []*ResponseError{}, Data: &JoinResponseData{}}
		if err := json.NewDecoder(rawResp.Body).Decode(resp); err != nil {
			return reconcile.Result{}, err
		}

		if resp.Errors != nil && len(resp.Errors) > 0 && !strings.Contains(resp.Errors[0].Message, "already joined") {
			return reconcile.Result{}, goerrors.New(resp.Errors[0].Message)
		}

		if resp.Data.JoinInstance == false {
			if resp.Errors != nil && len(resp.Errors) > 0 && strings.Contains(resp.Errors[0].Message, "already joined") {
				// return reconcile.Result{Requeue: true}, nil
				continue
			}
			return reconcile.Result{}, goerrors.New("JoinInstance == false")
		}
	}

	return reconcile.Result{}, nil
}
