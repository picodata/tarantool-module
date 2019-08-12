package role

import (
	"context"
	"fmt"

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
	"sigs.k8s.io/controller-runtime/pkg/controller/controllerutil"
	"sigs.k8s.io/controller-runtime/pkg/handler"
	"sigs.k8s.io/controller-runtime/pkg/manager"
	"sigs.k8s.io/controller-runtime/pkg/reconcile"
	logf "sigs.k8s.io/controller-runtime/pkg/runtime/log"
	"sigs.k8s.io/controller-runtime/pkg/source"
)

var log = logf.Log.WithName("controller_role")

/**
* USER ACTION REQUIRED: This is a scaffold file intended for the user to modify with their own Controller
* business logic.  Delete these comments after modifying this file.*
 */

// Add creates a new Role Controller and adds it to the Manager. The Manager will set fields on the Controller
// and Start it when the Manager is Started.
func Add(mgr manager.Manager) error {
	return add(mgr, newReconciler(mgr))
}

// newReconciler returns a new reconcile.Reconciler
func newReconciler(mgr manager.Manager) reconcile.Reconciler {
	return &ReconcileRole{client: mgr.GetClient(), scheme: mgr.GetScheme()}
}

// add adds a new Controller to mgr with r as the reconcile.Reconciler
func add(mgr manager.Manager, r reconcile.Reconciler) error {
	// Create a new controller
	c, err := controller.New("role-controller", mgr, controller.Options{Reconciler: r})
	if err != nil {
		return err
	}

	// Watch for changes to primary resource Role
	err = c.Watch(&source.Kind{Type: &tarantoolv1alpha1.Role{}}, &handler.EnqueueRequestForObject{})
	if err != nil {
		return err
	}

	// TODO(user): Modify this to be the types you create that are owned by the primary resource
	// Watch for changes to secondary resource Pods and requeue the owner Role
	err = c.Watch(&source.Kind{Type: &corev1.Pod{}}, &handler.EnqueueRequestForOwner{
		IsController: true,
		OwnerType:    &tarantoolv1alpha1.Role{},
	})
	if err != nil {
		return err
	}

	return nil
}

// blank assignment to verify that ReconcileRole implements reconcile.Reconciler
var _ reconcile.Reconciler = &ReconcileRole{}

// ReconcileRole reconciles a Role object
type ReconcileRole struct {
	// This client, initialized using mgr.Client() above, is a split client
	// that reads objects from the cache and writes to the apiserver
	client client.Client
	scheme *runtime.Scheme
}

// Reconcile reads that state of the cluster for a Role object and makes changes based on the state read
// and what is in the Role.Spec
// TODO(user): Modify this Reconcile function to implement your Controller logic.  This example creates
// a Pod as an example
// Note:
// The Controller will requeue the Request to be processed again if the returned error is non-nil or
// Result.Requeue is true, otherwise upon completion it will remove the work from the queue.
func (r *ReconcileRole) Reconcile(request reconcile.Request) (reconcile.Result, error) {
	reqLogger := log.WithValues("Request.Namespace", request.Namespace, "Request.Name", request.Name)
	reqLogger.Info("Reconciling Role")

	// Fetch the Role instance
	instance := &tarantoolv1alpha1.Role{}
	err := r.client.Get(context.TODO(), request.NamespacedName, instance)
	if err != nil {
		if errors.IsNotFound(err) {
			// Request object not found, could have been deleted after reconcile request.
			// Owned objects are automatically garbage collected. For additional cleanup logic use finalizers.
			// Return and don't requeue
			return reconcile.Result{}, nil
		}
		// Error reading the object - requeue the request.
		return reconcile.Result{}, err
	}

	var i int32
	for i = 0; i < *instance.Spec.Replicas; i++ {
		sts := &appsv1.StatefulSet{}
		sts.Name = fmt.Sprintf("%s-%d", instance.Name, i)
		sts.Namespace = request.Namespace
		if err := r.client.Get(context.TODO(), types.NamespacedName{Namespace: sts.Namespace, Name: sts.Name}, sts); err != nil {
			if errors.IsNotFound(err) {
				sts.Spec = instance.Spec.StorageTemplate
				if err := SetReplicasetUUID(sts); err != nil {
					return reconcile.Result{}, err
				}
				sts.Spec.Template.ObjectMeta.Labels["tarantool.io/replicaset-uuid"] = sts.Labels["tarantool.io/replicaset-uuid"]
				sts.Spec.Template.ObjectMeta.Labels["app.kubernetes.io/component"] = instance.Labels["app.kubernetes.io/component"]
				sts.Spec.Template.ObjectMeta.Labels["app.kubernetes.io/name"] = instance.Labels["app.kubernetes.io/part-of"]
				if err := controllerutil.SetControllerReference(instance, sts, r.scheme); err != nil {
					return reconcile.Result{}, err
				}
				if err := r.client.Create(context.TODO(), sts); err != nil {
					return reconcile.Result{}, err
				}
			}
		}
	}
	if len(instance.Spec.ServiceTemplate.Ports) > 0 {
		svc := &corev1.Service{}
		svc.Name = fmt.Sprintf("%s", instance.Name)
		svc.Namespace = request.Namespace
		if err := r.client.Get(context.TODO(), types.NamespacedName{Namespace: svc.Namespace, Name: svc.Name}, svc); err != nil {
			if errors.IsNotFound(err) {
				svc.Spec = instance.Spec.ServiceTemplate
				if err := r.client.Create(context.TODO(), svc); err != nil {
					return reconcile.Result{}, err
				}
			}
		}
	}

	return reconcile.Result{}, nil
}

func SetReplicasetUUID(o metav1.Object) error {
	labels := o.GetLabels()
	replicasetUUID, err := uuid.NewUUID()
	if err != nil {
		return err
	}
	if labels == nil {
		labels = make(map[string]string)
	}
	labels["tarantool.io/replicaset-uuid"] = replicasetUUID.String()
	o.SetLabels(labels)
	return nil
}
