package controller

import (
    "bou.ke/monkey"
    "context"
    "fmt"
    "net/http"
    "time"

    . "github.com/onsi/ginkgo"
    . "github.com/onsi/gomega"

    appsv1 "k8s.io/api/apps/v1"
    corev1 "k8s.io/api/core/v1"

    "sigs.k8s.io/controller-runtime/pkg/client"

    tarantoolv1alpha1 "github.com/tarantool/tarantool-operator/pkg/apis/tarantool/v1alpha1"
    helpers "github.com/tarantool/tarantool-operator/test/helpers"

    "github.com/machinebox/graphql"
    "github.com/tarantool/tarantool-operator/pkg/topology"
)

var _ = Describe("controllers integration testing", func() {
    var (
        ctx       = context.TODO()
        namespace = ClusterTestNamespace

        clusterId       = "" // setup for every spec in hook
        clusterName     = ""
        roleName        = ""
        rsTemplateName  = ""
        podTemplateName = ""
        serviceName     = ""
        stsName         = ""
        alias           = ""

        numReplicasets = int32(1)
        rolesToAssign  = "[\"vshard-router\",\"vshard-storage\"]"

        stopPortForwardChan chan struct{}
    )

    Context("controllers manage the cluster", func() {
        BeforeEach(func() {
            // setup variables for each spec
            clusterId = "test-cluster"
            clusterName = clusterId
            roleName = "test-role"
            rsTemplateName = "test-rs"
            stsName = fmt.Sprintf("%s-%d", roleName, 0)
            alias = fmt.Sprintf("%s-%d-%d", roleName, 0, 0)
            serviceName = "test-service"

            By("create new Cluster " + clusterName)
            cluster := helpers.NewCluster(helpers.ClusterParams{
                Name:      clusterName,
                Namespace: namespace,
                Id:        clusterId,
            })
            Expect(
                k8sClient.Create(ctx, &cluster),
            ).NotTo(HaveOccurred(), "failed to create Cluster resource")

            By("create new Role " + roleName)
            role := helpers.NewRole(helpers.RoleParams{
                Name:           roleName,
                Namespace:      namespace,
                ClusterId:      clusterId,
                RolesToAssign:  rolesToAssign,
                RsNum:          numReplicasets,
                RsTemplateName: rsTemplateName,
            })
            Expect(
                k8sClient.Create(ctx, &role),
            ).NotTo(HaveOccurred(), "failed to create test Role resource")

            By("create new Service " + serviceName)
            service := helpers.NewService(helpers.ServiceParams{
                Name:      serviceName,
                Namespace: namespace,
                RoleName:  roleName,
            })
            Expect(
                k8sClient.Create(ctx, &service),
            ).NotTo(HaveOccurred(), "failed to create test serviceRole resource")

            By("create new ReplicasetTemplate " + rsTemplateName)
            replicasetTemplate := helpers.NewReplicasetTemplate(
                helpers.ReplicasetTemplateParams{
                    Name:            rsTemplateName,
                    Namespace:       namespace,
                    ClusterId:       clusterId,
                    RoleName:        roleName,
                    RolesToAssign:   rolesToAssign,
                    PodTemplateName: podTemplateName,
                    ContainerName:   "t",
                    ContainerImage:  "vanyarock01/test-app:0.1.0-0-g68f6117",
                    ServiceName:     serviceName,
                },
            )
            Expect(
                k8sClient.Create(ctx, &replicasetTemplate),
            ).NotTo(HaveOccurred(), "failed to create test replicasetTemplate resource")

            // use workaround with port forwarding to pod
            // https://stackoverflow.com/questions/65739830/no-access-to-k8s-pod-by-internal-ip-from-the-envtest
            By("monkeypatch topology.NewBuiltInTopologyService function with port-forwarded url")
            monkey.Patch(topology.NewBuiltInTopologyService,
                func(opts ...topology.Option) *topology.BuiltInTopologyService {
                    s := &topology.BuiltInTopologyService{}
                    // after all options set mock url
                    opts = append(opts, topology.WithTopologyEndpoint("http://localhost:8081/admin/api"))
                    for _, opt := range opts {
                        opt(s)
                    }
                    return s
                },
            )

            By("wait until the pod goes to running")
            pod := &corev1.Pod{}
            Eventually(
                func() bool {
                    err := k8sClient.Get(ctx, client.ObjectKey{Name: alias, Namespace: namespace}, pod)
                    if err != nil {
                        return false
                    }
                    if pod.Status.Phase != corev1.PodRunning {
                        return false
                    }
                    return true
                },
                time.Second*40, time.Millisecond*500,
            ).Should(BeTrue())

            stopPortForwardChan = make(chan struct{}, 1)
            By("port-froward from pod to localhost:8081")
            PortForwardToPod(pod, 8081, 8081, stopPortForwardChan)
        })

        AfterEach(func() {
            close(stopPortForwardChan)
            monkey.Unpatch(topology.NewBuiltInTopologyService)

            By("delete cluster")
            cluster := &tarantoolv1alpha1.Cluster{}
            _ = k8sClient.Get(ctx, client.ObjectKey{Name: clusterName, Namespace: namespace}, cluster)
            _ = k8sClient.Delete(ctx, cluster)

            By("delete Service")
            service := &corev1.Service{}
            _ = k8sClient.Get(ctx, client.ObjectKey{Name: serviceName, Namespace: namespace}, service)
            _ = k8sClient.Delete(ctx, service)

            By("delete role")
            role := &tarantoolv1alpha1.Role{}
            _ = k8sClient.Get(ctx, client.ObjectKey{Name: roleName, Namespace: namespace}, role)
            _ = k8sClient.Delete(ctx, role)

            By("delete replicasetTemplate")
            rsTemplate := &tarantoolv1alpha1.ReplicasetTemplate{}
            _ = k8sClient.Get(ctx, client.ObjectKey{Name: rsTemplateName, Namespace: namespace}, rsTemplate)
            _ = k8sClient.Delete(ctx, rsTemplate)

            pod := &corev1.Pod{}
            Eventually(
                func() error {
                    return k8sClient.Get(ctx, client.ObjectKey{Name: alias, Namespace: namespace}, pod)
                },
                time.Second*20, time.Millisecond*500,
            ).Should(HaveOccurred())
        })

        It("performs actions with resources", func() {
            By("set cluster-id to Role annotations")
            role := &tarantoolv1alpha1.Role{}
            Eventually(
                func() bool {
                    err := k8sClient.Get(ctx, client.ObjectKey{Name: roleName, Namespace: namespace}, role)
                    if err != nil {
                        return false
                    }
                    if val, ok := role.GetAnnotations()["tarantool.io/cluster-id"]; ok && (val == clusterId) {
                        return true
                    }
                    return false
                },
                time.Second*20, time.Millisecond*500,
            ).Should(BeTrue())

            By("create cluster Service")
            svc := &corev1.Service{}
            Eventually(
                func() bool {
                    return (k8sClient.Get(ctx, client.ObjectKey{Name: clusterName, Namespace: namespace}, svc) == nil)
                },
                time.Second*20, time.Millisecond*500,
            ).Should(BeTrue())

            By("elect cluster leader")
            endpoints := &corev1.Endpoints{}
            Eventually(
                func() bool {
                    err := k8sClient.Get(ctx, client.ObjectKey{Name: clusterId, Namespace: namespace}, endpoints)
                    if err != nil {
                        return false
                    }
                    if _, ok := endpoints.GetAnnotations()["tarantool.io/leader"]; ok {
                        return true
                    }
                    return false
                },
                time.Second*80, time.Millisecond*500,
            ).Should(BeTrue())

            By("add instance-uuid to pod labels")
            pod := &corev1.Pod{}
            Eventually(
                func() bool {
                    err := k8sClient.Get(ctx, client.ObjectKey{Name: alias, Namespace: namespace}, pod)
                    if err != nil {
                        return false
                    }

                    if _, ok := pod.GetLabels()["tarantool.io/instance-uuid"]; ok {
                        return true
                    }

                    return false
                },
                time.Second*30, time.Millisecond*500,
            ).Should(BeTrue())

            By("add tarantool instance state (Ready) to pod labels")
            pod = &corev1.Pod{}
            Eventually(
                func() bool {
                    err := k8sClient.Get(ctx, client.ObjectKey{Name: alias, Namespace: namespace}, pod)
                    if err != nil {
                        return false
                    }

                    if val, ok := pod.GetLabels()["tarantool.io/instance-state"]; ok && (val == "joined") {
                        return true
                    }

                    return false
                },
                time.Second*30, time.Millisecond*500,
            ).Should(BeTrue())

            By("add isBootstrapped flag to pod annotations")
            sts := &appsv1.StatefulSet{}
            Eventually(
                func() bool {
                    err := k8sClient.Get(ctx, client.ObjectKey{Name: stsName, Namespace: namespace}, sts)
                    if err != nil {
                        return false
                    }

                    if val, ok := sts.GetAnnotations()["tarantool.io/isBootstrapped"]; !ok || val != "1" {
                        return false
                    }

                    return true
                },
                time.Second*20, time.Millisecond*500,
            ).Should(BeTrue())

            By("set cluster status to Ready state")
            cluster := &tarantoolv1alpha1.Cluster{}
            Eventually(
                func() bool {
                    err := k8sClient.Get(ctx, client.ObjectKey{Name: clusterName, Namespace: namespace}, cluster)
                    if err != nil {
                        return false
                    }

                    if cluster.Status.State != "Ready" {
                        return false
                    }

                    return true
                },
                time.Second*20, time.Millisecond*500,
            ).Should(BeTrue())
        })

        type (
            ReplicasetData struct {
                Alias string   `json:"alias"`
                Roles []string `json:"roles"`
            }

            QueryResponse struct {
                Replicasets []*ReplicasetData `json:"replicasets"`
            }
        )

        eventuallyWaitRoles := func(expectedRoles []string) {
            req := graphql.NewRequest(`query { replicasets { roles } }`)
            resp := &QueryResponse{}

            client := graphql.NewClient(
                "http://localhost:8081/admin/api",
                graphql.WithHTTPClient(&http.Client{Timeout: time.Duration(time.Second * 5)}),
            )
            Eventually(
                func() []string {
                    err := client.Run(ctx, req, resp)
                    if err != nil {
                        return []string{}
                    }
                    return resp.Replicasets[0].Roles
                },
                time.Second*20, time.Millisecond*500,
            ).Should(RolesMatcherObject(expectedRoles))
        }

        It("apply replicaset roles by create cluster", func() {
            eventuallyWaitRoles([]string{"vshard-router", "vshard-storage"})
        })

        It("apply replicaset roles after update rolesToAssign", func() {
            var (
                newRolesToAssign = "[\"vshard-router\",\"vshard-storage\",\"metrics\"]"
            )

            By("wait for the init roles to be applied")
            eventuallyWaitRoles([]string{"vshard-router", "vshard-storage"})

            By("update rolesToAssign")
            role := &tarantoolv1alpha1.Role{}
            Expect(
                k8sClient.Get(ctx, client.ObjectKey{Name: roleName, Namespace: namespace}, role),
            ).NotTo(HaveOccurred())

            rsTemplate := &tarantoolv1alpha1.ReplicasetTemplate{}
            Expect(
                k8sClient.Get(ctx, client.ObjectKey{Name: rsTemplateName, Namespace: namespace}, rsTemplate),
            ).NotTo(HaveOccurred())

            role.ObjectMeta.Annotations["tarantool.io/rolesToAssign"] = newRolesToAssign
            rsTemplate.ObjectMeta.Annotations["tarantool.io/rolesToAssign"] = newRolesToAssign
            rsTemplate.Spec.Template.ObjectMeta.Annotations["tarantool.io/rolesToAssign"] = newRolesToAssign

            Expect(
                k8sClient.Update(ctx, role),
            ).NotTo(HaveOccurred(), "failed to update Role")
            Expect(
                k8sClient.Update(ctx, rsTemplate),
            ).NotTo(HaveOccurred(), "failed to update ReplicasetTemplate")

            By("wait for the new roles to be applied")
            eventuallyWaitRoles([]string{"vshard-router", "vshard-storage", "metrics"})
        })
    })
})
