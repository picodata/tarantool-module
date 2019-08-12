package tarantoolcluster

import (
	"context"

	"github.com/google/uuid"
	tarantoolv1alpha1 "gitlab.com/tarantool/sandbox/tarantool-operator/pkg/apis/tarantool/v1alpha1"
	appsv1 "k8s.io/api/apps/v1"
	corev1 "k8s.io/api/core/v1"
	"k8s.io/apimachinery/pkg/api/errors"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/runtime"
	"k8s.io/apimachinery/pkg/types"
	"sigs.k8s.io/controller-runtime/pkg/client"
	"sigs.k8s.io/controller-runtime/pkg/controller"
	"sigs.k8s.io/controller-runtime/pkg/event"
	"sigs.k8s.io/controller-runtime/pkg/handler"
	"sigs.k8s.io/controller-runtime/pkg/manager"
	"sigs.k8s.io/controller-runtime/pkg/predicate"
	"sigs.k8s.io/controller-runtime/pkg/reconcile"
	logf "sigs.k8s.io/controller-runtime/pkg/runtime/log"
	"sigs.k8s.io/controller-runtime/pkg/source"
)

var log = logf.Log.WithName("controller_tarantoolcluster")

type TarantoolPodPredicate struct{}

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

// ShouldManage checks if Pod is a part of TarantoolCluster
func ShouldManage(o runtime.Object) bool {
	switch obj := o.(type) {
	case *corev1.Pod:
		val, ok := obj.GetObjectMeta().GetAnnotations()["tarantool.io/cluster"]
		if !ok {
			return false
		}
		if len(val) == 0 {
			return false
		}

		return true
	}
	return false
}

// ShouldFinalize checks if instance is subject of Tarantool Cluster finalization
func ShouldFinalize(o metav1.ObjectMetaAccessor) bool {
	for _, v := range o.GetObjectMeta().GetFinalizers() {
		if v == "tarantool.io/finalizable" {
			return true
		}
	}

	return false
}

func RemoveFinalizer(finalizers []string) []string {
	newFinalizers := []string{}
	for _, v := range finalizers {
		if v != "tarantool.io/finalizable" {
			newFinalizers = append(newFinalizers, v)
		}
	}
	return newFinalizers
}

func HasInstanceUUID(o metav1.ObjectMetaAccessor) bool {
	annotations := o.GetObjectMeta().GetAnnotations()
	if _, ok := annotations["tarantool.io/instance-uuid"]; ok {
		return true
	}

	return false
}

func SetInstanceUUID(o *corev1.Pod) *corev1.Pod {
	oldAnnotations := o.GetObjectMeta().GetAnnotations()
	instanceUUID, _ := uuid.NewUUID()
	oldAnnotations["tarantool.io/instance-uuid"] = instanceUUID.String()

	o.GetObjectMeta().SetAnnotations(oldAnnotations)
	return o
}

func HasReplicasetUUID(o metav1.ObjectMetaAccessor) bool {
	annotations := o.GetObjectMeta().GetAnnotations()
	if _, ok := annotations["tarantool.io/replicaset_uuid"]; ok {
		return true
	}

	return false
}



func SetReplicasetUUID(o *corev1.Pod) *corev1.Pod {
	oldAnnotations := o.GetObjectMeta().GetAnnotations()
	instanceUUID, _ := uuid.NewUUID()
	oldAnnotations["tarantool.io/replicaset_uuid"] = instanceUUID.String()

	o.GetObjectMeta().SetAnnotations(oldAnnotations)
	return o
}

func (p *TarantoolPodPredicate) Create(e event.CreateEvent) bool {
	return ShouldManage(e.Object)
}

func (p *TarantoolPodPredicate) Delete(e event.DeleteEvent) bool {
	return ShouldManage(e.Object)
}

func (p *TarantoolPodPredicate) Update(e event.UpdateEvent) bool {
	return ShouldManage(e.ObjectOld)
}

func (p *TarantoolPodPredicate) Generic(e event.GenericEvent) bool {
	return ShouldManage(e.Object)
}

var _ predicate.Predicate = &TarantoolPodPredicate{}

/**
* USER ACTION REQUIRED: This is a scaffold file intended for the user to modify with their own Controller
* business logic.  Delete these comments after modifying this file.*
 */

// Add creates a new TarantoolCluster Controller and adds it to the Manager. The Manager will set fields on the Controller
// and Start it when the Manager is Started.
func Add(mgr manager.Manager) error {
	return add(mgr, newReconciler(mgr))
}

// newReconciler returns a new reconcile.Reconciler
func newReconciler(mgr manager.Manager) reconcile.Reconciler {
	return &ReconcileTarantoolCluster{client: mgr.GetClient(), scheme: mgr.GetScheme()}
}

// add adds a new Controller to mgr with r as the reconcile.Reconciler
func add(mgr manager.Manager, r reconcile.Reconciler) error {
	// Create a new controller
	c, err := controller.New("tarantoolcluster-controller", mgr, controller.Options{Reconciler: r})
	if err != nil {
		return err
	}

	// Watch for changes to primary resource TarantoolCluster
	err = c.Watch(&source.Kind{Type: &tarantoolv1alpha1.TarantoolCluster{}}, &handler.EnqueueRequestForObject{})
	if err != nil {
		return err
	}

	// TODO(user): Modify this to be the types you create that are owned by the primary resource
	// Watch for changes to secondary resource Pods and requeue the owner TarantoolCluster
	err = c.Watch(&source.Kind{Type: &appsv1.Deployment{}}, &handler.EnqueueRequestsFromMapFunc{
		ToRequests: handler.ToRequestsFunc(func(a handler.MapObject) []reconcile.Request {
			return []reconcile.Request{
				{NamespacedName: types.NamespacedName{
					Namespace: a.Meta.GetNamespace(),
					Name:      "example-tarantoolcluster",
				}},
			}
		}),
	})
	if err != nil {
		return err
	}

	err = c.Watch(&source.Kind{Type: &appsv1.StatefulSet{}}, &handler.EnqueueRequestsFromMapFunc{
		ToRequests: handler.ToRequestsFunc(func(a handler.MapObject) []reconcile.Request {
			return []reconcile.Request{
				{NamespacedName: types.NamespacedName{
					Namespace: a.Meta.GetNamespace(),
					Name:      "example-tarantoolcluster",
				}},
			}
		}),
	})
	if err != nil {
		return err
	}

	return nil
}

// blank assignment to verify that ReconcileTarantoolCluster implements reconcile.Reconciler
var _ reconcile.Reconciler = &ReconcileTarantoolCluster{}

// ReconcileTarantoolCluster reconciles a TarantoolCluster object
type ReconcileTarantoolCluster struct {
	// This client, initialized using mgr.Client() above, is a split client
	// that reads objects from the cache and writes to the apiserver
	client client.Client
	scheme *runtime.Scheme
}

var count = 0

func (r *ReconcileTarantoolCluster) Reconcile(request reconcile.Request) (reconcile.Result, error) {
	reqLogger := log.WithValues("Request.Namespace", request.Namespace, "Request.Name", request.Name)
	reqLogger.Info("TarantoolCluster Reconciler: starting reconcile")

	cluster := &tarantoolv1alpha1.TarantoolCluster{
		Spec: tarantoolv1alpha1.TarantoolClusterSpec{
			Selector: metav1.LabelSelector{},
		},
	}
	err := r.client.Get(context.TODO(), request.NamespacedName, cluster)
	if err != nil {
		if errors.IsNotFound(err) {
			return reconcile.Result{}, nil
		}

		return reconcile.Result{}, err
	}

	selector, err := metav1.LabelSelectorAsSelector(&cluster.Spec.Selector)
	if err != nil {
		return reconcile.Result{}, err
	}

	// reqLogger.Info("Selector", "value", cluster.Spec.Selector)
	list := appsv1.DeploymentList{}
	err = r.client.List(context.TODO(), &client.ListOptions{LabelSelector: selector}, &list)
	if err != nil {
		if errors.IsNotFound(err) {
			return reconcile.Result{}, nil
		}

		return reconcile.Result{}, err
	}

	if len(list.Items) > 0 {
		for _, item := range list.Items {
			item.SetOwnerReferences([]metav1.OwnerReference{
				*metav1.NewControllerRef(cluster, tarantoolv1alpha1.SchemeGroupVersion.WithKind("TarantoolCluster")),
			})
			if err := r.client.Update(context.TODO(), &item); err != nil {
				return reconcile.Result{}, err
			}
		}
	}

	reqLogger.Info("deployments", "list", list)

	reqLogger.Info("TarantoolCluster Reconciler: finished reconcile")

	return reconcile.Result{}, nil
}

// Reconcile reads that state of the cluster for a TarantoolCluster object and makes changes based on the state read
// and what is in the TarantoolCluster.Spec
// TODO(user): Modify this Reconcile function to implement your Controller logic.  This example creates
// a Pod as an example
// Note:
// The Controller will requeue the Request to be processed again if the returned error is non-nil or
// Result.Requeue is true, otherwise upon completion it will remove the work from the queue.
// func (r *ReconcileTarantoolCluster) Reconcile(request reconcile.Request) (reconcile.Result, error) {
// 	reqLogger := log.WithValues("Request.Namespace", request.Namespace, "Request.Name", request.Name)
// 	reqLogger.Info("Reconciling TarantoolCluster")

// 	// Fetch the TarantoolCluster instance
// 	instance := &corev1.Pod{}
// 	err := r.client.Get(context.TODO(), request.NamespacedName, instance)

// 	if err != nil {
// 		if errors.IsNotFound(err) {
// 			// Request object not found, could have been deleted after reconcile request.
// 			// Owned objects are automatically garbage collected. For additional cleanup logic use finalizers.
// 			// Return and don't requeue
// 			return reconcile.Result{}, nil
// 		}
// 		// Error reading the object - requeue the request.
// 		return reconcile.Result{}, err
// 	}

// 	if instance.GetObjectMeta().GetDeletionTimestamp().IsZero() {
		if !HasInstanceUUID(instance) {
			instance = SetInstanceUUID(instance)

			if err := r.client.Update(context.TODO(), instance); err != nil {
				return reconcile.Result{}, err
			}

			return reconcile.Result{}, nil
		}

// 		if !HasReplicasetUUID(instance) {
// 			instance = SetReplicasetUUID(instance)

// 			if err := r.client.Update(context.TODO(), instance); err != nil {
// 				return reconcile.Result{}, err
// 			}

// 			return reconcile.Result{}, nil
// 		}

		podIP := instance.Status.PodIP
		if len(podIP) == 0 {
			return reconcile.Result{}, goerrors.New("Waiting for pod")
		}
		advURI := fmt.Sprintf("%s:3301", podIP)
// 		instanceUUID, _ := instance.GetAnnotations()["tarantool.io/instance_uuid"]
		replicasetUUID, _ := instance.GetAnnotations()["tarantool.io/replicaset_uuid"]
		req := fmt.Sprintf("mutation {join_instance: join_server(uri: \\\"%s\\\",instance_uuid: \\\"%s\\\",replicaset_uuid: \\\"%s\\\",roles: [\\\"storage\\\"],timeout: 10)}", advURI, instanceUUID, replicasetUUID)

		j := fmt.Sprintf("{\"query\": \"%s\"}", req)

		rawResp, err := http.Post("http://127.0.0.1:8081/admin/api", "application/json", strings.NewReader(j))
		if err != nil {
			reqLogger.Error(err, "join err")
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
				return reconcile.Result{}, nil
			}
			return reconcile.Result{}, goerrors.New("JoinInstance == false")
		}

// 		reqLogger.Info("Join response", "resp", resp)
// 	} else {
// 		if ShouldFinalize(instance) {
// 			reqLogger.Info("DO FINALIZE", "pod", instance.Name)
// 			req := fmt.Sprintf("mutation {expel_instance:expel_server(uuid:\\\"%s\\\")}", instance.GetAnnotations()["tarantool.io/instance_uuid"])
// 			j := fmt.Sprintf("{\"query\": \"%s\"}", req)
// 			rawResp, err := http.Post("http://127.0.0.1:8081/admin/api", "application/json", strings.NewReader(j))
// 			if err != nil {
// 				reqLogger.Error(err, "expel err")
// 				return reconcile.Result{}, err
// 			}
// 			defer rawResp.Body.Close()

// 			resp := &ExpelResponse{Errors: []*ResponseError{}, Data: &ExpelResponseData{}}
// 			if err := json.NewDecoder(rawResp.Body).Decode(resp); err != nil {
// 				return reconcile.Result{}, err
// 			}

// 			if resp.Data.ExpelInstance == false && (resp.Errors == nil || len(resp.Errors) == 0) {
// 				return reconcile.Result{}, goerrors.New("Shit happened")
// 			}

// 			if len(resp.Errors) > 0 && strings.Contains(resp.Errors[0].Message, "dead") {
// 				instance.ObjectMeta.Finalizers = RemoveFinalizer(instance.GetObjectMeta().GetFinalizers())

// 				if err = r.client.Update(context.TODO(), instance); err != nil {
// 					return reconcile.Result{}, err
// 				}

// 				return reconcile.Result{}, goerrors.New(resp.Errors[0].Message)
// 			}

// 			if len(resp.Errors) > 0 && strings.Contains(resp.Errors[0].Message, "already expelled") {
// 				instance.ObjectMeta.Finalizers = RemoveFinalizer(instance.GetObjectMeta().GetFinalizers())

// 				if err = r.client.Update(context.TODO(), instance); err != nil {
// 					return reconcile.Result{}, err
// 				}

// 				return reconcile.Result{}, goerrors.New(resp.Errors[0].Message)
// 			}

// 			if len(resp.Errors) > 0 {
// 				return reconcile.Result{}, goerrors.New(resp.Errors[0].Message)
// 			}

// 			instance.ObjectMeta.Finalizers = RemoveFinalizer(instance.GetObjectMeta().GetFinalizers())

// 			if err = r.client.Update(context.TODO(), instance); err != nil {
// 				return reconcile.Result{}, err
// 			}
// 		}
// 	}

// 	// Pod already exists - don't requeue
// 	reqLogger.Info("Skip reconcile: Pod already exists", "Pod.IP", instance.Status.PodIP, "Pod.Name", instance.Name)
// 	return reconcile.Result{}, nil
// }

// newPodForCR returns a busybox pod with the same name/namespace as the cr
func newPodForCR(cr *tarantoolv1alpha1.TarantoolCluster) *appsv1.StatefulSet {
	// labels := map[string]string{
	// 	"app": cr.Name,
	// }
	sts := &appsv1.StatefulSet{}
	sts.Namespace = cr.Namespace
	sts.Name = cr.Name
	// sts.Spec = cr.Spec.ReplicasetTemplate
	return sts
}
