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
)

// RoleReconciler reconciles a Role object
type RoleReconciler struct {
	client.Client
	Scheme *runtime.Scheme
}

//+kubebuilder:rbac:groups=tarantool.io,resources=roles,verbs=get;list;watch;create;update;patch;delete
//+kubebuilder:rbac:groups=tarantool.io,resources=roles/status,verbs=get;update;patch
//+kubebuilder:rbac:groups=tarantool.io,resources=roles/finalizers,verbs=update

// Reconcile is part of the main kubernetes reconciliation loop which aims to
// move the current state of the cluster closer to the desired state.
// TODO(user): Modify the Reconcile function to compare the state specified by
// the Role object against the actual cluster state, and then
// perform operations to make the cluster state reflect the state specified by
// the user.
//
// For more details, check Reconcile and its Result here:
// - https://pkg.go.dev/sigs.k8s.io/controller-runtime@v0.10.0/pkg/reconcile
func (r *RoleReconciler) Reconcile(ctx context.Context, req ctrl.Request) (ctrl.Result, error) {
	reqLogger := log.FromContext(ctx)
	reqLogger.Info("Reconciling Role")

	role := &tarantooliov1alpha1.Role{}
	err := r.Get(context.TODO(), req.NamespacedName, role)
	if err != nil {
		if errors.IsNotFound(err) {
			return ctrl.Result{}, nil
		}
		return ctrl.Result{}, err
	}

	if len(role.GetOwnerReferences()) == 0 {
		return ctrl.Result{}, fmt.Errorf("Orphan role %s", role.GetName())
	}

	templateSelector, err := metav1.LabelSelectorAsSelector(role.Spec.Selector)
	if err != nil {
		return ctrl.Result{}, err
	}

	reqLogger.Info("Got selector", "selector", templateSelector)

	stsSelector := &metav1.LabelSelector{
		MatchLabels: role.GetLabels(),
	}
	s, err := metav1.LabelSelectorAsSelector(stsSelector)
	if err != nil {
		return ctrl.Result{}, err
	}

	stsList := &appsv1.StatefulSetList{}
	if err := r.List(context.TODO(), stsList, &client.ListOptions{LabelSelector: s}); err != nil {
		return ctrl.Result{}, err
	}

	// ensure num of statefulsets matches user expectations
	if len(stsList.Items) > int(*role.Spec.NumReplicasets) {
		reqLogger.Info("Role", "more instances", *role.Spec.NumReplicasets)
		for i := len(stsList.Items); i > int(*role.Spec.NumReplicasets); i-- {
			sts := &appsv1.StatefulSet{}
			sts.Name = fmt.Sprintf("%s-%d", role.Name, i-1)
			sts.Namespace = req.Namespace
			reqLogger.Info("ROLE DOWNSCALE", "will remove", sts.Name)

			if err := r.Get(context.TODO(), types.NamespacedName{Namespace: sts.Namespace, Name: sts.Name}, sts); err != nil {
				if errors.IsNotFound(err) {
					continue
				}
				return ctrl.Result{}, err
			}

			stsAnnotations := sts.GetAnnotations()
			if stsAnnotations["tarantool.io/scheduledDelete"] == "1" {
				reqLogger.Info("statefulset is ready for deletion")
			}

			// if err := r.client.Delete(context.TODO(), sts); err != nil {
			// 	return reconcile.Result{}, err
			// }
		}
	}

	templateList := &tarantooliov1alpha1.ReplicasetTemplateList{}
	if err := r.List(context.TODO(), templateList, &client.ListOptions{LabelSelector: templateSelector}); err != nil {
		return ctrl.Result{}, err
	}

	if len(templateList.Items) == 0 {
		return ctrl.Result{}, fmt.Errorf("no template")
	}

	template := templateList.Items[0]

	if len(stsList.Items) < int(*role.Spec.NumReplicasets) {
		for i := 0; i < int(*role.Spec.NumReplicasets); i++ {
			sts := &appsv1.StatefulSet{}
			sts.Name = fmt.Sprintf("%s-%d", role.Name, i)
			sts.Namespace = req.Namespace

			if err := r.Get(context.TODO(), types.NamespacedName{Namespace: sts.Namespace, Name: sts.Name}, sts); err != nil {
				sts = CreateStatefulSetFromTemplate(ctx, i, fmt.Sprintf("%s-%d", role.Name, i), role, &template)
				if err := controllerutil.SetControllerReference(role, sts, r.Scheme); err != nil {
					return ctrl.Result{}, err
				}
				if err := r.Create(context.TODO(), sts); err != nil {
					return ctrl.Result{}, err
				}
			}
		}
	}

	for _, sts := range stsList.Items {
		if template.Spec.Replicas != sts.Spec.Replicas {
			reqLogger.Info("Updating replicas count")
			sts.Spec.Replicas = template.Spec.Replicas
			if err := r.Update(context.TODO(), &sts); err != nil {
				return ctrl.Result{}, err
			}
		}

		if template.Spec.Template.Spec.Containers[0].Image != sts.Spec.Template.Spec.Containers[0].Image {
			reqLogger.Info("Updating container image")
			sts.Spec.Template.Spec.Containers[0].Image = template.Spec.Template.Spec.Containers[0].Image
			if err := r.Update(context.TODO(), &sts); err != nil {
				return ctrl.Result{}, err
			}
		}

		sts.Spec.Template.Spec.Containers[0].Env = template.Spec.Template.Spec.Containers[0].Env
		reqLogger.Info("Env variables", "vars", sts.Spec.Template.Spec.Containers[0].Env)
		if err := r.Update(context.TODO(), &sts); err != nil {
			return ctrl.Result{}, err
		}

		if templateRolesToAssign, ok := template.ObjectMeta.Annotations["tarantool.io/rolesToAssign"]; ok {
			// check rolesToAssign from annotations
			if templateRolesToAssign != sts.ObjectMeta.Annotations["tarantool.io/rolesToAssign"] {
				reqLogger.Info("Updating replicaset rolesToAssign",
					"from", sts.ObjectMeta.Annotations["tarantool.io/rolesToAssign"],
					"to", templateRolesToAssign)

				sts.ObjectMeta.Annotations["tarantool.io/rolesToAssign"] = templateRolesToAssign
				sts.Spec.Template.Annotations["tarantool.io/rolesToAssign"] = templateRolesToAssign

				if err := r.Update(context.TODO(), &sts); err != nil {
					return ctrl.Result{}, err
				}
			}
		} else {
			// check rolesToAssign from labels (deprecated)
			templateRolesToAssignFromLabels, ok := template.ObjectMeta.Labels["tarantool.io/rolesToAssign"]
			if ok && templateRolesToAssignFromLabels != sts.ObjectMeta.Labels["tarantool.io/rolesToAssign"] {
				reqLogger.Info("Updating replicaset rolesToAssign from labels",
					"from", sts.ObjectMeta.Labels["tarantool.io/rolesToAssign"],
					"to", templateRolesToAssignFromLabels)

				sts.ObjectMeta.Labels["tarantool.io/rolesToAssign"] = templateRolesToAssignFromLabels
				sts.Spec.Template.Labels["tarantool.io/rolesToAssign"] = templateRolesToAssignFromLabels

				if err := r.Update(context.TODO(), &sts); err != nil {
					return ctrl.Result{}, err
				}
			}
		}
	}

	return ctrl.Result{}, nil
}

// SetupWithManager sets up the controller with the Manager.
func (r *RoleReconciler) SetupWithManager(mgr ctrl.Manager) error {
	return ctrl.NewControllerManagedBy(mgr).
		For(&tarantooliov1alpha1.Role{}).
		Watches(&source.Kind{Type: &tarantooliov1alpha1.Role{}}, &handler.EnqueueRequestForObject{}).
		Watches(&source.Kind{Type: &appsv1.StatefulSet{}}, &handler.EnqueueRequestForOwner{
			IsController: true,
			OwnerType:    &tarantooliov1alpha1.Role{},
		}).
		Watches(&source.Kind{Type: &tarantooliov1alpha1.ReplicasetTemplate{}}, handler.EnqueueRequestsFromMapFunc(func(a client.Object) []reconcile.Request {
			roleList := &tarantooliov1alpha1.RoleList{}
			if err := r.Client.List(context.TODO(), roleList, &client.ListOptions{}); err != nil {
				mgr.GetLogger().Info("FUCK")
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
		})).
		Complete(r)
}

// CreateStatefulSetFromTemplate .
func CreateStatefulSetFromTemplate(ctx context.Context, replicasetNumber int, name string, role *tarantooliov1alpha1.Role, rs *tarantooliov1alpha1.ReplicasetTemplate) *appsv1.StatefulSet {
	reqLogger := log.FromContext(ctx)

	reqLogger.Info("RST", "IS:", *rs)
	sts := &appsv1.StatefulSet{
		Spec: *rs.Spec,
	}
	reqLogger.Info("STS", "IS:", *sts)

	sts.Name = name
	sts.Namespace = role.GetNamespace()
	sts.ObjectMeta.Labels = role.GetLabels()

	sts.Spec.UpdateStrategy = appsv1.StatefulSetUpdateStrategy{Type: "OnDelete"}

	for k, v := range role.GetLabels() {
		sts.Spec.Template.Labels[k] = v
	}

	privileged := false

	sts.Spec.Template.Spec.Containers[0].SecurityContext = &corev1.SecurityContext{
		Privileged: &privileged,
	}

	sts.Spec.ServiceName = role.GetAnnotations()["tarantool.io/cluster-id"]
	replicasetUUID := uuid.NewSHA1(space, []byte(sts.GetName()))
	sts.ObjectMeta.Labels["tarantool.io/replicaset-uuid"] = replicasetUUID.String()
	sts.ObjectMeta.Labels["tarantool.io/vshardGroupName"] = role.GetLabels()["tarantool.io/role"]

	if sts.ObjectMeta.Annotations == nil {
		sts.ObjectMeta.Annotations = make(map[string]string)
	}

	sts.ObjectMeta.Annotations["tarantool.io/isBootstrapped"] = "0"
	sts.ObjectMeta.Annotations["tarantool.io/replicaset-weight"] = "100"

	sts.Spec.Template.Labels["tarantool.io/replicaset-uuid"] = replicasetUUID.String()
	sts.Spec.Template.Labels["tarantool.io/vshardGroupName"] = role.GetLabels()["tarantool.io/role"]

	return sts
}
