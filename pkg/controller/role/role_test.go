package role

import (
	"context"
	"fmt"
	"time"

	. "github.com/onsi/ginkgo"
	. "github.com/onsi/gomega"

	helpers "github.com/tarantool/tarantool-operator/test/helpers"

	appsv1 "k8s.io/api/apps/v1"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"

	tarantoolv1alpha1 "github.com/tarantool/tarantool-operator/pkg/apis/tarantool/v1alpha1"

	"sigs.k8s.io/controller-runtime/pkg/client"
)

var _ = Describe("role_controller unit testing", func() {
	var (
		namespace = "default"
		ctx       = context.TODO()
	)

	Describe("role_controller should react to the sts-template change and update the sts", func() {
		Context("update rolesToAssign annotation in sts-template", func() {
			var (
				roleName       = "" // setup for every spec in hook
				rsTemplateName = ""
				stsName        = ""

				clusterId = "t"

				defaultRolesToAssign = "[\"A\",\"B\"]"
				newRolesToAssign     = "[\"A\",\"B\",\"C\"]"
			)

			BeforeEach(func() {
				// setup variables for each spec
				roleName = fmt.Sprintf("test-role-%s", RandStringRunes(4))
				rsTemplateName = fmt.Sprintf("test-rs-%s", RandStringRunes(4))
				stsName = fmt.Sprintf("%s-%d", roleName, 0)

				By("create new Role " + roleName)
				role := helpers.NewRole(helpers.RoleParams{
					Name:           roleName,
					Namespace:      namespace,
					RolesToAssign:  defaultRolesToAssign,
					RsNum:          int32(1),
					RsTemplateName: rsTemplateName,
					ClusterId:      clusterId,
				})
				// mock owner reference
				role.SetOwnerReferences([]metav1.OwnerReference{
					{
						APIVersion: "v0",
						Kind:       "mockRef",
						Name:       "mockRef",
						UID:        "-",
					},
				})
				Expect(k8sClient.Create(ctx, &role)).NotTo(HaveOccurred(), "failed to create Role")

				By("create new ReplicasetTemplate " + rsTemplateName)
				rsTemplate := helpers.NewReplicasetTemplate(helpers.ReplicasetTemplateParams{
					Name:          rsTemplateName,
					Namespace:     namespace,
					RoleName:      roleName,
					RolesToAssign: defaultRolesToAssign,
				})
				Expect(k8sClient.Create(ctx, &rsTemplate)).NotTo(HaveOccurred(), "failed to create ReplicasetTemplate")
			})

			AfterEach(func() {
				By("remove role object " + roleName)
				role := &tarantoolv1alpha1.Role{}
				Expect(
					k8sClient.Get(ctx, client.ObjectKey{Name: roleName, Namespace: namespace}, role),
				).NotTo(HaveOccurred(), "failed to get Role")

				Expect(k8sClient.Delete(ctx, role)).NotTo(HaveOccurred(), "failed to delete Role")

				By("remove ReplicasetTemplate object " + rsTemplateName)
				rsTemplate := &tarantoolv1alpha1.ReplicasetTemplate{}
				Expect(
					k8sClient.Get(ctx, client.ObjectKey{Name: rsTemplateName, Namespace: namespace}, rsTemplate),
				)

				Expect(k8sClient.Delete(ctx, rsTemplate)).NotTo(HaveOccurred(), "failed to delete Role")
			})

			It("set rolesToAssign by creating sts", func() {
				By("get sts")
				sts := &appsv1.StatefulSet{}
				Eventually(
					func() bool {
						return k8sClient.Get(ctx, client.ObjectKey{Name: stsName, Namespace: namespace}, sts) == nil
					},
					time.Second*10, time.Millisecond*500,
				).Should(BeTrue())

				By("check roleToAssign in sts")
				Expect(sts.ObjectMeta.Annotations["tarantool.io/rolesToAssign"]).To(Equal(defaultRolesToAssign))
				Expect(sts.Spec.Template.Annotations["tarantool.io/rolesToAssign"]).To(Equal(defaultRolesToAssign))
			})

			It("set roleToAssign by updating sts-template", func() {
				By("update rolesToAssign annotations in ReplicasetTemplate")
				rsTemplate := &tarantoolv1alpha1.ReplicasetTemplate{}
				Expect(
					k8sClient.Get(ctx, client.ObjectKey{Name: rsTemplateName, Namespace: namespace}, rsTemplate),
				).NotTo(HaveOccurred(), "failed to get ReplicasetTemplate")

				rsTemplate.ObjectMeta.Annotations["tarantool.io/rolesToAssign"] = newRolesToAssign
				rsTemplate.Spec.Template.ObjectMeta.Annotations["tarantool.io/rolesToAssign"] = newRolesToAssign
				Expect(
					k8sClient.Update(ctx, rsTemplate),
				).NotTo(HaveOccurred(), "failed to update ReplicasetTemplate")

				By("update rolesToAssign annotation in Role")
				role := &tarantoolv1alpha1.Role{}
				Expect(
					k8sClient.Get(ctx, client.ObjectKey{Name: roleName, Namespace: namespace}, role),
				).NotTo(HaveOccurred(), "failed to get Role")

				role.ObjectMeta.Annotations["tarantool.io/rolesToAssign"] = newRolesToAssign
				Expect(
					k8sClient.Update(ctx, role),
				).NotTo(HaveOccurred(), "failed to update Role")

				By("check roleToAssign in sts")
				sts := &appsv1.StatefulSet{}
				Eventually(
					func() bool {
						err := k8sClient.Get(ctx, client.ObjectKey{Name: stsName, Namespace: namespace}, sts)
						if err != nil {
							return false
						}

						if sts.ObjectMeta.Annotations["tarantool.io/rolesToAssign"] == newRolesToAssign &&
							sts.Spec.Template.ObjectMeta.Annotations["tarantool.io/rolesToAssign"] == newRolesToAssign {
							return true
						}

						return false
					},
					time.Second*10, time.Millisecond*500,
				).Should(BeTrue())
			})
		})
	})
})
