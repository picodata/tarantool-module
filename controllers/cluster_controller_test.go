package controllers

import (
	"context"
	"fmt"
	"math/rand"
	"time"

	. "github.com/onsi/ginkgo"
	. "github.com/onsi/gomega"

	helpers "github.com/tarantool/tarantool-operator/test/helpers"

	corev1 "k8s.io/api/core/v1"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"

	"sigs.k8s.io/controller-runtime/pkg/client"
)

var letterRunes = []rune("abcdefghijklmnopqrstuvwxyz")

func RandStringRunes(n int) string {
	b := make([]rune, n)
	for i := range b {
		b[i] = letterRunes[rand.Intn(len(letterRunes))]
	}
	return string(b)
}

var _ = Describe("cluster_controller unit testing", func() {
	var (
		ctx         = context.TODO()
		namespace   = "test"
		clusterName = "test"
		clusterId   = "test"
		ns          = &corev1.Namespace{
			ObjectMeta: metav1.ObjectMeta{
				Name: namespace,
			},
		}
		cartridge = helpers.NewCartridge(helpers.CartridgeParams{
			Namespace:   namespace,
			ClusterName: clusterName,
			ClusterID:   clusterId,
		})
	)

	Describe("cluster_controller manage cluster resources", func() {
		BeforeEach(func() {
			Expect(k8sClient.Create(ctx, ns)).
				NotTo(
					HaveOccurred(),
					fmt.Sprintf("failed to create Namespace %s", ns.GetName()),
				)

			Expect(k8sClient.Create(ctx, cartridge.Cluster)).
				NotTo(
					HaveOccurred(),
					fmt.Sprintf("failed to create Cluster %s", cartridge.Cluster.GetName()),
				)

			for _, role := range cartridge.Roles {
				Expect(k8sClient.Create(ctx, role)).
					NotTo(
						HaveOccurred(),
						fmt.Sprintf("failed to create Role %s", role.GetName()),
					)
			}

			for _, rs := range cartridge.ReplicasetTemplates {
				Expect(k8sClient.Create(ctx, rs)).
					NotTo(
						HaveOccurred(),
						fmt.Sprintf("failed to create ReplicasetTemplate %s", rs.GetName()),
					)
			}

			for _, svc := range cartridge.Services {
				Expect(k8sClient.Create(ctx, svc)).
					NotTo(
						HaveOccurred(),
						fmt.Sprintf("failed to create Service %s", svc.GetName()),
					)
			}
		})

		AfterEach(func() {
			By("remove Namespace object " + namespace)
			ns := &corev1.Namespace{}
			Expect(
				k8sClient.Get(ctx, client.ObjectKey{Name: namespace}, ns),
			).NotTo(HaveOccurred(), "failed to get Namespace")

			Expect(k8sClient.Delete(ctx, ns)).NotTo(HaveOccurred(), "failed to delete Namespace")
		})

		Context("manage cluster leader: tarantool instance accepting admin requests", func() {
			It("change the leader if the previous one does not exist", func() {
				By("get the chosen leader")
				ep := corev1.Endpoints{}
				Eventually(
					func() bool {
						err := k8sClient.Get(ctx, client.ObjectKey{Name: clusterName, Namespace: namespace}, &ep)
						if err != nil {
							return false
						}

						if ep.GetAnnotations()["tarantool.io/leader"] != "" {
							return true
						}

						return false
					},
					2*time.Minute,
					500*time.Millisecond,
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
					2*time.Minute,
					500*time.Millisecond,
				).Should(BeTrue())
			})
		})
	})

	Describe("cluster_controller unit testing functions", func() {
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
