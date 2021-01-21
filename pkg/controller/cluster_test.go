package controller

import (
    "bou.ke/monkey"
    "context"
    "fmt"
    "time"

    . "github.com/onsi/ginkgo"
    . "github.com/onsi/gomega"

    appsv1 "k8s.io/api/apps/v1"
    corev1 "k8s.io/api/core/v1"
    metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"

    "k8s.io/apimachinery/pkg/api/resource"
    "k8s.io/apimachinery/pkg/util/intstr"

    "sigs.k8s.io/controller-runtime/pkg/client"

    tarantoolv1alpha1 "github.com/tarantool/tarantool-operator/pkg/apis/tarantool/v1alpha1"
    "github.com/tarantool/tarantool-operator/pkg/topology"
)

var _ = Describe("cluster_controller testing", func() {
    var (
        clusterId   = "test-cluster"
        clusterName = clusterId

        numReplicasets         = int32(1)
        roleName               = "test-role"
        replicasetTemplateName = "test-template"
        rolesToAssign          = "[\"vshard-router\",\"vshard-storage\"]"

        serviceName = roleName

        numReplicas            = int32(1)
        dnsOptionsValue        = "1"
        fsGroup                = int64(1000)
        terminationGracePeriod = int64(10)

        stsName = fmt.Sprintf("%s-%d", roleName, 0)
        alias   = fmt.Sprintf("%s-%d-%d", roleName, 0, 0)
        advHost = fmt.Sprintf("%s.%s.%s.svc.cluster.local", alias, clusterId, ClusterTestNamespace)
        advUri  = fmt.Sprintf("%s:3301", advHost)
    )

    ctx := context.TODO()

    Context("create Cluster", func() {
        It("cluster should be created", func() {
            cluster := &tarantoolv1alpha1.Cluster{
                ObjectMeta: metav1.ObjectMeta{
                    Name:      clusterName,
                    Namespace: ClusterTestNamespace,
                },
                Spec: tarantoolv1alpha1.ClusterSpec{
                    Selector: &metav1.LabelSelector{
                        MatchLabels: map[string]string{
                            "tarantool.io/cluster-id": clusterId,
                        },
                    },
                },
            }
            Expect(k8sClient.Create(ctx, cluster)).NotTo(HaveOccurred(), "failed to create cluster resource")
        })
    })

    Context("create Role", func() {
        It("role should be created", func() {
            role := &tarantoolv1alpha1.Role{
                ObjectMeta: metav1.ObjectMeta{
                    Name:      roleName,
                    Namespace: ClusterTestNamespace,
                    Labels: map[string]string{
                        "tarantool.io/cluster-id": clusterId,
                        "tarantool.io/role":       roleName,
                    },
                    Annotations: map[string]string{
                        "tarantool.io/rolesToAssign": rolesToAssign,
                    },
                },
                Spec: tarantoolv1alpha1.RoleSpec{
                    NumReplicasets: &numReplicasets,
                    Selector: &metav1.LabelSelector{
                        MatchLabels: map[string]string{
                            "tarantool.io/replicaset-template": replicasetTemplateName,
                        },
                    },
                },
            }
            Expect(k8sClient.Create(ctx, role)).NotTo(HaveOccurred(), "failed to create test Role resource")
        })
    })

    Context("create Service", func() {
        It("service should be created", func() {
            service := &corev1.Service{
                ObjectMeta: metav1.ObjectMeta{
                    Name:      serviceName,
                    Namespace: ClusterTestNamespace,
                    Labels: map[string]string{
                        "tarantool.io/role": roleName,
                    },
                },

                Spec: corev1.ServiceSpec{
                    Ports: []corev1.ServicePort{
                        {
                            Name:     "web",
                            Protocol: corev1.ProtocolTCP,
                            Port:     int32(8081),
                        },
                        {
                            Name:     "app",
                            Protocol: corev1.ProtocolTCP,
                            Port:     int32(3301),
                        },
                    },
                    Selector: map[string]string{
                        "tarantool.io/role": roleName,
                    },
                },
            }

            Expect(k8sClient.Create(ctx, service)).NotTo(HaveOccurred(), "failed to create test serviceRole resource")
        })
    })

    Context("create ReplicasetTemplate", func() {
        It("replicasetTemplate should be created", func() {
            replicasetTemplate := &tarantoolv1alpha1.ReplicasetTemplate{
                ObjectMeta: metav1.ObjectMeta{
                    Name:      replicasetTemplateName,
                    Namespace: ClusterTestNamespace,
                    Labels: map[string]string{
                        "tarantool.io/cluster-id":          clusterId,
                        "tarantool.io/replicaset-template": replicasetTemplateName,
                        "tarantool.io/role":                roleName,
                        "tarantool.io/useVshardGroups":     "0",
                    },
                    Annotations: map[string]string{
                        "tarantool.io/rolesToAssign": rolesToAssign,
                    },
                },
                Spec: &appsv1.StatefulSetSpec{
                    Replicas:    &numReplicas,
                    ServiceName: serviceName,
                    Selector: &metav1.LabelSelector{
                        MatchLabels: map[string]string{
                            "tarantool.io/pod-template": "test-role-pod-template",
                        },
                    },
                    VolumeClaimTemplates: []corev1.PersistentVolumeClaim{
                        {
                            ObjectMeta: metav1.ObjectMeta{
                                Name: "www",
                            },
                            Spec: corev1.PersistentVolumeClaimSpec{
                                AccessModes: []corev1.PersistentVolumeAccessMode{
                                    corev1.ReadWriteOnce,
                                },
                                Resources: corev1.ResourceRequirements{
                                    Requests: corev1.ResourceList{
                                        "storage": *resource.NewQuantity(1*1024*1024*1024, resource.BinarySI),
                                    },
                                },
                            },
                        },
                    },
                    Template: corev1.PodTemplateSpec{
                        ObjectMeta: metav1.ObjectMeta{
                            Labels: map[string]string{
                                "tarantool.io/cluster-id":          clusterId,
                                "tarantool.io/cluster-domain-name": "cluster.local",
                                "tarantool.io/pod-template":        "test-role-pod-template",
                                "tarantool.io/useVshardGroups":     "0",
                                "environment":                      "dev",
                            },
                            Annotations: map[string]string{
                                "tarantool.io/rolesToAssign": rolesToAssign,
                            },
                        },
                        Spec: corev1.PodSpec{
                            TerminationGracePeriodSeconds: &terminationGracePeriod,
                            DNSConfig: &corev1.PodDNSConfig{
                                Options: []corev1.PodDNSConfigOption{
                                    {
                                        Name:  "ndots",
                                        Value: &dnsOptionsValue,
                                    },
                                },
                            },
                            SecurityContext: &corev1.PodSecurityContext{
                                FSGroup: &fsGroup,
                            },
                            Containers: []corev1.Container{
                                {
                                    Name:  "test",
                                    Image: "vanyarock01/test-app:0.1.0-0-g68f6117",
                                    VolumeMounts: []corev1.VolumeMount{
                                        {
                                            Name:      "www",
                                            MountPath: "/var/lib/tarantool",
                                        },
                                    },
                                    SecurityContext: &corev1.SecurityContext{
                                        Capabilities: &corev1.Capabilities{
                                            Add: []corev1.Capability{
                                                "SYS_ADMIN",
                                            },
                                        },
                                    },
                                    Resources: corev1.ResourceRequirements{
                                        Limits: corev1.ResourceList{
                                            "cpu":    *resource.NewMilliQuantity(1000, resource.DecimalSI),
                                            "memory": *resource.NewQuantity(256*1024*1024, resource.BinarySI),
                                        },
                                    },
                                    Ports: []corev1.ContainerPort{
                                        {
                                            Name:          "app",
                                            Protocol:      corev1.ProtocolTCP,
                                            ContainerPort: int32(3301),
                                        },
                                        {
                                            Name:          "app-udp",
                                            Protocol:      corev1.ProtocolUDP,
                                            ContainerPort: int32(3301),
                                        },
                                        {
                                            Name:          "http",
                                            Protocol:      corev1.ProtocolTCP,
                                            ContainerPort: int32(8081),
                                        },
                                    },
                                    Env: []corev1.EnvVar{
                                        {
                                            Name:  "ENVIRONMENT",
                                            Value: "dev",
                                        },
                                        {
                                            Name:  "TARANTOOL_INSTANCE_NAME",
                                            Value: alias,
                                        },
                                        {
                                            Name:  "TARANTOOL_ALIAS",
                                            Value: alias,
                                        },
                                        {
                                            Name:  "TARANTOOL_MEMTX_MEMORY",
                                            Value: "268435456",
                                        },
                                        {
                                            Name:  "TARANTOOL_BUCKET_COUNT",
                                            Value: "30000",
                                        },
                                        {
                                            Name:  "TARANTOOL_WORKDIR",
                                            Value: "/var/lib/tarantool",
                                        },
                                        {
                                            Name:  "TARANTOOL_ADVERTISE_TMP",
                                            Value: alias,
                                        },
                                        {
                                            Name:  "TARANTOOL_ADVERTISE_HOST",
                                            Value: advHost,
                                        },
                                        {
                                            Name:  "TARANTOOL_ADVERTISE_URI",
                                            Value: advUri,
                                        },
                                        {
                                            Name:  "TARANTOOL_PROBE_URI_TIMEOUT",
                                            Value: "60",
                                        },
                                        {
                                            Name:  "TARANTOOL_HTTP_PORT",
                                            Value: "8081",
                                        },
                                    },
                                    ReadinessProbe: &corev1.Probe{
                                        Handler: corev1.Handler{
                                            TCPSocket: &corev1.TCPSocketAction{
                                                Port: intstr.IntOrString{
                                                    Type:   intstr.String,
                                                    StrVal: "http",
                                                },
                                            },
                                        },
                                        InitialDelaySeconds: int32(15),
                                        PeriodSeconds:       int32(10),
                                    },
                                },
                            },
                        },
                    },
                },
            }
            Expect(
                k8sClient.Create(ctx, replicasetTemplate),
            ).NotTo(HaveOccurred(), "failed to create test replicasetTemplate resource")
        })
    })

    Context("controller performs actions with resources", func() {
        It("set cluster-id to Role annotations", func() {
            role := &tarantoolv1alpha1.Role{}
            Eventually(
                func() bool {
                    err := k8sClient.Get(ctx, client.ObjectKey{Name: roleName, Namespace: ClusterTestNamespace}, role)
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
        })

        It("create cluster Service", func() {
            svc := &corev1.Service{}
            Eventually(
                func() bool {
                    return (k8sClient.Get(ctx, client.ObjectKey{Name: clusterName, Namespace: ClusterTestNamespace}, svc) == nil)
                },
                time.Second*20, time.Millisecond*500,
            ).Should(BeTrue())
        })

        It("elect cluster leader", func() {
            endpoints := &corev1.Endpoints{}
            Eventually(
                func() bool {
                    err := k8sClient.Get(ctx, client.ObjectKey{Name: clusterId, Namespace: ClusterTestNamespace}, endpoints)
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
        })

        It("workaround with port forwarding to pod", func() {
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

            By("get pod")
            pod := &corev1.Pod{}
            Eventually(
                func() bool {
                    return k8sClient.Get(ctx, client.ObjectKey{Name: alias, Namespace: ClusterTestNamespace}, pod) == nil
                },
                time.Second*20, time.Millisecond*500,
            ).Should(BeTrue())

            By("port-froward from pod to localhost:8081")
            PortForwardToPod(pod, 8081, 8081, make(chan struct{}, 1))
        })

        It("add instance-uuid to pod labels", func() {
            pod := &corev1.Pod{}
            Eventually(
                func() bool {
                    err := k8sClient.Get(ctx, client.ObjectKey{Name: alias, Namespace: ClusterTestNamespace}, pod)
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
        })

        It("add tarantool instance state (Ready) to pod labels", func() {
            pod := &corev1.Pod{}
            Eventually(
                func() bool {
                    err := k8sClient.Get(ctx, client.ObjectKey{Name: alias, Namespace: ClusterTestNamespace}, pod)
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
        })

        It("add isBootstrapped flag to pod annotations", func() {
            sts := &appsv1.StatefulSet{}
            Eventually(
                func() bool {
                    err := k8sClient.Get(ctx, client.ObjectKey{Name: stsName, Namespace: ClusterTestNamespace}, sts)
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
        })

        It("set cluster status to Ready state", func() {
            cluster := &tarantoolv1alpha1.Cluster{}
            Eventually(
                func() bool {
                    err := k8sClient.Get(ctx, client.ObjectKey{Name: clusterName, Namespace: ClusterTestNamespace}, cluster)
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

        It("after all", func() {
            By("unpatching topology.NewBuiltInTopologyService function")
            monkey.Unpatch(topology.NewBuiltInTopologyService)
        })
    })
})
