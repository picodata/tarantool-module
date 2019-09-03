package role

import (
	"context"
	"fmt"

	goerrors "errors"

	"github.com/google/uuid"
	tarantoolv1alpha1 "github.com/tarantool/tarantool-operator/pkg/apis/tarantool/v1alpha1"
	appsv1 "k8s.io/api/apps/v1"
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
var space = uuid.MustParse("C4FA9F56-A49A-4384-8BEE-9A476725973F")

type ResponseError struct {
	Message string `json:"message"`
}

type ExpelResponseData struct {
	ExpelInstance bool `json:"expel_instance"`
}
type ExpelResponse struct {
	Errors []*ResponseError   `json:"errors,omitempty"`
	Data   *ExpelResponseData `json:"data,omitempty"`
}

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

	err = c.Watch(&source.Kind{Type: &appsv1.StatefulSet{}}, &handler.EnqueueRequestForOwner{
		IsController: true,
		OwnerType:    &tarantoolv1alpha1.Role{},
	})
	if err != nil {
		return err
	}

	err = c.Watch(&source.Kind{Type: &tarantoolv1alpha1.ReplicasetTemplate{}}, &handler.EnqueueRequestsFromMapFunc{
		ToRequests: handler.ToRequestsFunc(func(a handler.MapObject) []reconcile.Request {
			rec := r.(*ReconcileRole)
			roleList := &tarantoolv1alpha1.RoleList{}
			if err := rec.client.List(context.TODO(), &client.ListOptions{}, roleList); err != nil {
				log.Info("FUCK")
			}

			res := []reconcile.Request{}
			for _, role := range roleList.Items {
				res = append(res, reconcile.Request{
					NamespacedName: types.NamespacedName{
						Name:      role.GetName(),
						Namespace: role.GetNamespace(),
					},
				})
			}
			return res
		}),
	})

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

	role := &tarantoolv1alpha1.Role{}
	err := r.client.Get(context.TODO(), request.NamespacedName, role)
	if err != nil {
		if errors.IsNotFound(err) {
			return reconcile.Result{}, nil
		}
		return reconcile.Result{}, err
	}

	if len(role.GetOwnerReferences()) == 0 {
		return reconcile.Result{}, goerrors.New(fmt.Sprintf("Orphan role %s", role.GetName()))
	}

	templateSelector, err := metav1.LabelSelectorAsSelector(role.Spec.Selector)
	if err != nil {
		return reconcile.Result{}, err
	}

	reqLogger.Info("Got selector", "selector", templateSelector)

	stsSelector := &metav1.LabelSelector{
		MatchLabels: role.GetLabels(),
	}
	s, err := metav1.LabelSelectorAsSelector(stsSelector)
	if err != nil {
		return reconcile.Result{}, err
	}

	stsList := &appsv1.StatefulSetList{}
	if err := r.client.List(context.TODO(), &client.ListOptions{LabelSelector: s}, stsList); err != nil {
		return reconcile.Result{}, err
	}

	// ensure num of statefulsets matches user expectations
	if len(stsList.Items) > int(*role.Spec.NumReplicasets) {
		reqLogger.Info("Role", "more instances", *role.Spec.NumReplicasets)
		for i := len(stsList.Items); i > int(*role.Spec.NumReplicasets); i-- {
			sts := &appsv1.StatefulSet{}
			sts.Name = fmt.Sprintf("%s-%d", role.Name, i-1)
			sts.Namespace = request.Namespace
			reqLogger.Info("ROLE DOWNSCALE", "will remove", sts.Name)
			if err := r.client.Get(context.TODO(), types.NamespacedName{Namespace: sts.Namespace, Name: sts.Name}, sts); err != nil {
				if errors.IsNotFound(err) {
					continue
				}
				return reconcile.Result{}, err
			}

			if err := r.client.Delete(context.TODO(), sts); err != nil {
				return reconcile.Result{}, err
			}
		}
	}

	templateList := &tarantoolv1alpha1.ReplicasetTemplateList{}
	if err := r.client.List(context.TODO(), &client.ListOptions{LabelSelector: templateSelector}, templateList); err != nil {
		return reconcile.Result{}, err
	}
	if len(templateList.Items) == 0 {
		return reconcile.Result{}, goerrors.New("no template")
	}
	template := templateList.Items[0]

	if len(stsList.Items) < int(*role.Spec.NumReplicasets) {
		for i := 0; i < int(*role.Spec.NumReplicasets); i++ {
			sts := &appsv1.StatefulSet{}
			sts.Name = fmt.Sprintf("%s-%d", role.Name, i)
			sts.Namespace = request.Namespace

			if err := r.client.Get(context.TODO(), types.NamespacedName{Namespace: sts.Namespace, Name: sts.Name}, sts); err != nil {
				sts = CreateStatefulSetFromTemplate(fmt.Sprintf("%s-%d", role.Name, i), role, &template)
				if err := controllerutil.SetControllerReference(role, sts, r.scheme); err != nil {
					return reconcile.Result{}, err
				}
				if err := r.client.Create(context.TODO(), sts); err != nil {
					return reconcile.Result{}, err
				}
			}
		}
	}

	for _, sts := range stsList.Items {
		if template.Spec.Replicas != sts.Spec.Replicas {
			sts.Spec.Replicas = template.Spec.Replicas
			if err := r.client.Update(context.TODO(), &sts); err != nil {
				return reconcile.Result{}, err
			}
		}
	}

	return reconcile.Result{}, nil
}

func CreateStatefulSetFromTemplate(name string, role *tarantoolv1alpha1.Role, rs *tarantoolv1alpha1.ReplicasetTemplate) *appsv1.StatefulSet {
	sts := &appsv1.StatefulSet{
		Spec: *rs.Spec,
	}
	sts.Name = name
	sts.Namespace = role.GetNamespace()
	sts.ObjectMeta.Labels = role.GetLabels()
	for k, v := range role.GetLabels() {
		sts.Spec.Template.Labels[k] = v
	}
	sts.Spec.ServiceName = role.GetAnnotations()["tarantool.io/cluster-id"]
	replicasetUUID := uuid.NewSHA1(space, []byte(sts.GetName()))
	sts.ObjectMeta.Labels["tarantool.io/replicaset-uuid"] = replicasetUUID.String()
	sts.Spec.Template.Labels["tarantool.io/replicaset-uuid"] = replicasetUUID.String()

	return sts
}

func RemoveFinalizer(finalizers []string) []string {
	newFinalizers := []string{}
	for _, v := range finalizers {
		if v != "tarantool.io/replicaset" {
			newFinalizers = append(newFinalizers, v)
		}
	}
	return newFinalizers
}
