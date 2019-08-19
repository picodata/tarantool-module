package role

import (
	"context"
	"fmt"

	goerrors "errors"

	"github.com/google/uuid"
	tarantoolv1alpha1 "gitlab.com/tarantool/sandbox/tarantool-operator/pkg/apis/tarantool/v1alpha1"
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

	// TODO(user): Modify this to be the types you create that are owned by the primary resource
	// Watch for changes to secondary resource Pods and requeue the owner Role
	err = c.Watch(&source.Kind{Type: &appsv1.StatefulSet{}}, &handler.EnqueueRequestForOwner{
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
	role := &tarantoolv1alpha1.Role{}
	err := r.client.Get(context.TODO(), request.NamespacedName, role)
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

	if len(stsList.Items) > int(*role.Spec.Replicas) {
		reqLogger.Info("Role", "more instances", *role.Spec.Replicas)
		for i := len(stsList.Items); i > int(*role.Spec.Replicas); i-- {
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

	if len(stsList.Items) < int(*role.Spec.Replicas) {
		for i := 0; i < int(*role.Spec.Replicas); i++ {
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

	// if !role.GetDeletionTimestamp().IsZero() {
	// 	list := appsv1.StatefulSetList{}
	// 	selector := labels.NewSelector()
	// 	requirement, err := labels.NewRequirement("tarantool.io/cluster-id", selection.Equals, []string{role.Labels["tarantool.io/cluster-id"]})
	// 	if err != nil {
	// 		return reconcile.Result{}, err
	// 	}
	// 	selector = selector.Add(*requirement)

	// 	requirement, err = labels.NewRequirement("app.kubernetes.io/component", selection.Equals, []string{role.Labels["app.kubernetes.io/component"]})
	// 	if err != nil {
	// 		return reconcile.Result{}, err
	// 	}
	// 	selector = selector.Add(*requirement)

	// 	if err := r.client.List(context.TODO(), &client.ListOptions{LabelSelector: selector}, &list); err != nil {
	// 		if errors.IsNotFound(err) {
	// 			return reconcile.Result{}, err
	// 		}
	// 	}
	// }

	// list := appsv1.StatefulSetList{}
	// selector := labels.NewSelector()
	// requirement, err := labels.NewRequirement("tarantool.io/cluster-id", selection.Equals, []string{role.Labels["tarantool.io/cluster-id"]})
	// if err != nil {
	// 	return reconcile.Result{}, err
	// }
	// selector = selector.Add(*requirement)

	// requirement, err = labels.NewRequirement("app.kubernetes.io/component", selection.Equals, []string{role.Labels["app.kubernetes.io/component"]})
	// if err != nil {
	// 	return reconcile.Result{}, err
	// }
	// selector = selector.Add(*requirement)

	// if err := r.client.List(context.TODO(), &client.ListOptions{LabelSelector: selector}, &list); err != nil {
	// 	if errors.IsNotFound(err) {
	// 		return reconcile.Result{}, err
	// 	}
	// }

	// reqLogger.Info("list", "len", len(list.Items))

	// for _, sts := range list.Items {
	// 	if !sts.GetDeletionTimestamp().IsZero() {
	// 		podList := corev1.PodList{}
	// 		selector := labels.NewSelector()
	// 		requirement, err := labels.NewRequirement("tarantool.io/cluster-id", selection.Equals, []string{sts.Labels["tarantool.io/cluster-id"]})
	// 		if err != nil {
	// 			return reconcile.Result{}, err
	// 		}
	// 		selector = selector.Add(*requirement)
	// 		requirement, err = labels.NewRequirement("tarantool.io/replicaset-uuid", selection.Equals, []string{sts.Labels["tarantool.io/replicaset-uuid"]})
	// 		if err != nil {
	// 			return reconcile.Result{}, err
	// 		}
	// 		selector = selector.Add(*requirement)
	// 		if err := r.client.List(context.TODO(), &client.ListOptions{LabelSelector: selector}, &podList); err != nil {
	// 			if errors.IsNotFound(err) {
	// 				return reconcile.Result{}, err
	// 			}
	// 		}

	// 		if len(podList.Items) > 0 {
	// 			for _, pod := range podList.Items {
	// 				reqLogger.Info("DO FINALIZE", "pod", pod.GetName())
	// 				req := fmt.Sprintf("mutation {expel_instance:expel_server(uuid:\\\"%s\\\")}", pod.GetLabels()["tarantool.io/instance-uuid"])
	// 				j := fmt.Sprintf("{\"query\": \"%s\"}", req)
	// 				rawResp, err := http.Post("http://127.0.0.1:8081/admin/api", "application/json", strings.NewReader(j))
	// 				if err != nil {
	// 					reqLogger.Error(err, "expel err")
	// 					return reconcile.Result{}, err
	// 				}
	// 				defer rawResp.Body.Close()

	// 				resp := &ExpelResponse{Errors: []*ResponseError{}, Data: &ExpelResponseData{}}
	// 				if err := json.NewDecoder(rawResp.Body).Decode(resp); err != nil {
	// 					return reconcile.Result{}, err
	// 				}
	// 				reqLogger.Info("RESP", "resp", resp)

	// 				if resp.Data.ExpelInstance == true || (resp.Data.ExpelInstance == false && resp.Errors != nil && len(resp.Errors) > 0 && strings.Contains(resp.Errors[0].Message, "already expelled")) {
	// 					reqLogger.Info("Shit happened", "resp", "Already expelled")
	// 					continue
	// 				} else {
	// 					return reconcile.Result{}, goerrors.New(resp.Errors[0].Message)
	// 				}

	// 				reqLogger.Info("FINISH DO FINALIZE")
	// 			}

	// 			sts.Finalizers = RemoveFinalizer(sts.Finalizers)
	// 			if err := r.client.Update(context.TODO(), &sts); err != nil {
	// 				return reconcile.Result{}, err
	// 			}
	// 		}
	// 	}
	// }

	// if len(list.Items) < int(*role.Spec.Replicas) {
	// 	var i int32
	// 	for i = 0; i < *role.Spec.Replicas; i++ {
	// 		sts := &appsv1.StatefulSet{}
	// 		sts.Name = fmt.Sprintf("%s-%d", role.Name, i)
	// 		sts.Namespace = request.Namespace
	// 		if err := r.client.Get(context.TODO(), types.NamespacedName{Namespace: sts.Namespace, Name: sts.Name}, sts); err != nil {
	// 			if errors.IsNotFound(err) {
	// 				sts.Spec = role.Spec.StorageTemplate
	// 				//					sts.Finalizers = append(sts.Finalizers, "tarantool.io/finalizable")
	// 				if err := SetReplicasetUUID(sts); err != nil {
	// 					return reconcile.Result{}, err
	// 				}
	// 				if err := tntutils.SetTarantoolClusterID(sts, role.Labels["tarantool.io/cluster-id"]); err != nil {
	// 					return reconcile.Result{}, err
	// 				}
	// 				if err := tntutils.SetComponent(sts, role.Labels["app.kubernetes.io/component"]); err != nil {
	// 					return reconcile.Result{}, err
	// 				}
	// 				sts.Finalizers = append(sts.Finalizers, "tarantool.io/replicaset")
	// 				sts.Spec.Template.ObjectMeta.Labels["tarantool.io/replicaset-uuid"] = sts.Labels["tarantool.io/replicaset-uuid"]
	// 				sts.Spec.Template.ObjectMeta.Labels["tarantool.io/cluster-id"] = sts.Labels["tarantool.io/cluster-id"]
	// 				sts.Spec.Template.ObjectMeta.Labels["app.kubernetes.io/component"] = role.Labels["app.kubernetes.io/component"]
	// 				sts.Spec.Template.ObjectMeta.Labels["app.kubernetes.io/name"] = role.Labels["app.kubernetes.io/part-of"]
	// 				if err := controllerutil.SetControllerReference(role, sts, r.scheme); err != nil {
	// 					return reconcile.Result{}, err
	// 				}
	// 				if err := r.client.Create(context.TODO(), sts); err != nil {
	// 					return reconcile.Result{}, err
	// 				}
	// 			}
	// 		}
	// 	}
	// }

	// if len(list.Items) > int(*role.Spec.Replicas) {
	// 	reqLogger.Info("Role", "more instances", *role.Spec.Replicas)
	// 	var i int32
	// 	for i = int32(len(list.Items)); i > *role.Spec.Replicas; i-- {
	// 		sts := &appsv1.StatefulSet{}
	// 		sts.Name = fmt.Sprintf("%s-%d", role.Name, i-1)
	// 		sts.Namespace = request.Namespace
	// 		reqLogger.Info("ROLE DOWNSCALE", "will remove", sts.Name)
	// 		if err := r.client.Get(context.TODO(), types.NamespacedName{Namespace: sts.Namespace, Name: sts.Name}, sts); err != nil {
	// 			return reconcile.Result{}, err
	// 		}

	// 		if err := r.client.Delete(context.TODO(), sts); err != nil {
	// 			return reconcile.Result{}, err
	// 		}
	// 	}
	// }

	// if len(role.Spec.ServiceTemplate.Ports) > 0 {
	// 	svc := &corev1.Service{}
	// 	svc.Name = fmt.Sprintf("%s", role.Name)
	// 	svc.Namespace = request.Namespace
	// 	if err := r.client.Get(context.TODO(), types.NamespacedName{Namespace: svc.Namespace, Name: svc.Name}, svc); err != nil {
	// 		if errors.IsNotFound(err) {
	// 			svc.Spec = role.Spec.ServiceTemplate
	// 			if err := r.client.Create(context.TODO(), svc); err != nil {
	// 				return reconcile.Result{}, err
	// 			}
	// 		}
	// 	}
	// }

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
	replicasetUUID, _ := uuid.NewUUID()
	sts.ObjectMeta.Labels["tarantool.io/replicaset-uuid"] = replicasetUUID.String()
	sts.Spec.Template.Labels["tarantool.io/replicaset-uuid"] = replicasetUUID.String()
	return sts
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

func RemoveFinalizer(finalizers []string) []string {
	newFinalizers := []string{}
	for _, v := range finalizers {
		if v != "tarantool.io/replicaset" {
			newFinalizers = append(newFinalizers, v)
		}
	}
	return newFinalizers
}
