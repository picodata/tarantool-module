package cluster

import (
	"context"
	"fmt"
	"time"

	. "github.com/onsi/ginkgo"
	. "github.com/onsi/gomega"

	helpers "github.com/tarantool/tarantool-operator/test/helpers"

	corev1 "k8s.io/api/core/v1"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"

	tarantoolv1alpha1 "github.com/tarantool/tarantool-operator/pkg/apis/tarantool/v1alpha1"

	"sigs.k8s.io/controller-runtime/pkg/client"
)

var _ = Describe("cluster_controller unit testing", func() {
	var (
		namespace = "default"
		ctx       = context.TODO()

		roleName       = "" // setup for every spec in hook
		rsTemplateName = ""

		clusterName = "test"
		clusterId   = clusterName

		defaultRolesToAssign = "[\"A\",\"B\"]"
	)

	Describe("cluster_controller manage cluster resources", func() {
		BeforeEach(func() {
			// setup variables for each spec
			roleName = fmt.Sprintf("test-role-%s", RandStringRunes(4))
			rsTemplateName = fmt.Sprintf("test-rs-%s", RandStringRunes(4))

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

			By("create new Cluster " + clusterName)
			cluster := helpers.NewCluster(helpers.ClusterParams{
				Name:      clusterName,
				Namespace: namespace,
				Id:        clusterId,
			})
			Expect(k8sClient.Create(ctx, &cluster)).NotTo(HaveOccurred(), "failed to create Cluster")
		})

		AfterEach(func() {
			By("remove role object " + roleName)
			role := &tarantoolv1alpha1.Role{}
			Expect(
				k8sClient.Get(ctx, client.ObjectKey{Name: roleName, Namespace: namespace}, role),
			).NotTo(HaveOccurred(), "failed to get Role")

			Expect(k8sClient.Delete(ctx, role)).NotTo(HaveOccurred(), "failed to delete Role")

			By("remove Cluster object " + clusterName)
			cluster := &tarantoolv1alpha1.Cluster{}
			Expect(
				k8sClient.Get(ctx, client.ObjectKey{Name: clusterName, Namespace: namespace}, cluster),
			).NotTo(HaveOccurred(), "failed to get Cluster")

			Expect(k8sClient.Delete(ctx, cluster)).NotTo(HaveOccurred(), "failed to delete Cluster")
		})

		Context("manage cluster leader: tarantool instance accepting admin requests", func() {
			BeforeEach(func() {
				By("create cluster endpoints")
				ep := corev1.Endpoints{
					ObjectMeta: metav1.ObjectMeta{
						Name:      clusterId,
						Namespace: namespace,
					},
					Subsets: []corev1.EndpointSubset{
						{
							Addresses: []corev1.EndpointAddress{
								{IP: "1.1.1.1"},
								{IP: "2.2.2.2"},
								{IP: "3.3.3.3"},
							},
						},
					},
				}
				Expect(k8sClient.Create(ctx, &ep)).NotTo(HaveOccurred(), "failed to create cluster endpoints")
			})

			AfterEach(func() {
				ep := corev1.Endpoints{}
				Expect(
					k8sClient.Get(ctx, client.ObjectKey{Name: clusterId, Namespace: namespace}, &ep),
				).NotTo(HaveOccurred(), "failed to get cluster endpoints")

				Expect(k8sClient.Delete(ctx, &ep)).NotTo(HaveOccurred(), "failed to delete endpoints")
			})

			It("change the leader if the previous one does not exist", func() {
				By("get the chosen leader")
				ep := corev1.Endpoints{}
				Eventually(
					func() bool {
						err := k8sClient.Get(ctx, client.ObjectKey{Name: clusterId, Namespace: namespace}, &ep)
						if err != nil {
							return false
						}

						if ep.GetAnnotations()["tarantool.io/leader"] != "" {
							return true
						}

						return false
					},
					time.Second*10, time.Millisecond*500,
				).Should(BeTrue())

				By("save old leader")
				oldLeader := ep.GetAnnotations()["tarantool.io/leader"]

				By("set all new IP addresses")
				ep.Subsets = []corev1.EndpointSubset{
					{
						Addresses: []corev1.EndpointAddress{
							{IP: "4.4.4.4"},
							{IP: "5.5.5.5"},
							{IP: "6.6.6.6"},
						},
					},
				}
				Expect(k8sClient.Update(ctx, &ep)).NotTo(HaveOccurred(), "failed to update cluster endpoints")

				By("check that the leader has changed")
				Eventually(
					func() bool {
						err := k8sClient.Get(ctx, client.ObjectKey{Name: clusterId, Namespace: namespace}, &ep)
						if err != nil {
							return false
						}

						if ep.GetAnnotations()["tarantool.io/leader"] != oldLeader {
							return true
						}
						return false
					},
					time.Second*10, time.Millisecond*500,
				).Should(BeTrue())
			})
		})
	})

	Describe("cluster_contriller unit testing functions", func() {
		Describe("function IsLeaderExists must check for existence of leader in annotation of cluster Endpoints", func() {
			Context("positive cases (leader exist)", func() {
				It("should return True if leader assigned and exist", func() {
					leaderIP := "1.1.1.1"

					ep := &corev1.Endpoints{
						ObjectMeta: metav1.ObjectMeta{
							Name:      "name",
							Namespace: "namespace",
							Annotations: map[string]string{
								"tarantool.io/leader": fmt.Sprintf("%s:8081", leaderIP),
							},
						},
						Subsets: []corev1.EndpointSubset{
							{
								Addresses: []corev1.EndpointAddress{
									{IP: leaderIP},
								},
							},
						},
					}
					Expect(IsLeaderExists(ep)).To(BeTrue())
				})
			})

			Context("negative cases (leader does not exist)", func() {
				It("should return False if leader not assigned", func() {
					ep := &corev1.Endpoints{
						ObjectMeta: metav1.ObjectMeta{
							Name:      "name",
							Namespace: "namespace",
						},
					}
					Expect(IsLeaderExists(ep)).To(BeFalse())
				})

				It("should return False if leader assigned, but IP not exists", func() {
					ep := &corev1.Endpoints{
						ObjectMeta: metav1.ObjectMeta{
							Name:      "name",
							Namespace: "namespace",
							Annotations: map[string]string{
								"tarantool.io/leader": "6.6.6.6:8081",
							},
						},
						Subsets: []corev1.EndpointSubset{
							{
								Addresses: []corev1.EndpointAddress{
									{IP: "0.0.0.0"},
								},
							},
						},
					}
					Expect(IsLeaderExists(ep)).To(BeFalse())
				})
			})
		})
	})
})
