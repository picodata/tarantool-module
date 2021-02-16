package helpers

import (
	"fmt"

	appsv1 "k8s.io/api/apps/v1"
	corev1 "k8s.io/api/core/v1"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"

	tarantoolv1alpha1 "github.com/tarantool/tarantool-operator/pkg/apis/tarantool/v1alpha1"

	"k8s.io/apimachinery/pkg/api/resource"
)

type ClusterParams struct {
	Name      string
	Namespace string
	Id        string
}

type RoleParams struct {
	Name           string
	Namespace      string
	ClusterId      string
	RolesToAssign  string
	RsNum          int32
	RsTemplateName string
}

type ReplicasetTemplateParams struct {
	Name            string
	Namespace       string
	ClusterId       string
	RoleName        string
	RolesToAssign   string
	PodTemplateName string
	ContainerName   string
	ContainerImage  string
	ServiceName     string
	EnvVars         map[string]string
}

type ServiceParams struct {
	Name      string
	Namespace string
	RoleName  string
}

// Create new tarantoolv1alpha1.Cluster
func NewCluster(params ClusterParams) tarantoolv1alpha1.Cluster {
	return tarantoolv1alpha1.Cluster{
		ObjectMeta: metav1.ObjectMeta{
			Name:      params.Name,
			Namespace: params.Namespace,
		},
		Spec: tarantoolv1alpha1.ClusterSpec{
			Selector: &metav1.LabelSelector{
				MatchLabels: map[string]string{
					"tarantool.io/cluster-id": params.Id,
				},
			},
		},
	}
}

// Create new tarantoolv1alpha1.Role
func NewRole(params RoleParams) tarantoolv1alpha1.Role {
	return tarantoolv1alpha1.Role{
		ObjectMeta: metav1.ObjectMeta{
			Name:      params.Name,
			Namespace: params.Namespace,
			Labels: map[string]string{
				"tarantool.io/cluster-id": params.ClusterId,
				"tarantool.io/role":       params.Name,
			},
			Annotations: map[string]string{
				"tarantool.io/rolesToAssign": params.RolesToAssign,
			},
		},
		Spec: tarantoolv1alpha1.RoleSpec{
			NumReplicasets: &params.RsNum,
			Selector: &metav1.LabelSelector{
				MatchLabels: map[string]string{
					"tarantool.io/replicaset-template": params.RsTemplateName,
				},
			},
		},
	}
}

// Create new tarantoolv1alpha1.ReplicasetTemplate
func NewReplicasetTemplate(params ReplicasetTemplateParams) tarantoolv1alpha1.ReplicasetTemplate {
	var (
		replicasNum = int32(1)
		alias       = fmt.Sprintf("%s-%d-%d", params.RoleName, 0, 0)
		advHost     = fmt.Sprintf("%s.%s.%s.svc.cluster.local", alias, params.ClusterId, params.Namespace)
		advUri      = fmt.Sprintf("%s:3301", advHost)
	)

	envs := map[string]string{
		"ENVIRONMENT":                 "dev",
		"TARANTOOL_INSTANCE_NAME":     alias,
		"TARANTOOL_ALIAS":             alias,
		"TARANTOOL_MEMTX_MEMORY":      "268435456",
		"TARANTOOL_BUCKET_COUNT":      "30000",
		"TARANTOOL_WORKDIR":           "/var/lib/tarantool",
		"TARANTOOL_ADVERTISE_TMP":     alias,
		"TARANTOOL_ADVERTISE_HOST":    advHost,
		"TARANTOOL_ADVERTISE_URI":     advUri,
		"TARANTOOL_PROBE_URI_TIMEOUT": "60",
		"TARANTOOL_HTTP_PORT":         "8081",
	}

	for name, val := range params.EnvVars {
		envs[name] = val
	}

	vars := []corev1.EnvVar{}
	for name, val := range envs {
		vars = append(vars, corev1.EnvVar{Name: name, Value: val})
	}

	return tarantoolv1alpha1.ReplicasetTemplate{
		ObjectMeta: metav1.ObjectMeta{
			Name:      params.Name,
			Namespace: params.Namespace,
			Labels: map[string]string{
				"tarantool.io/cluster-id":          params.ClusterId,
				"tarantool.io/replicaset-template": params.Name,
				"tarantool.io/role":                params.RoleName,
				"tarantool.io/useVshardGroups":     "0",
			},
			Annotations: map[string]string{
				"tarantool.io/rolesToAssign": params.RolesToAssign,
			},
		},
		Spec: &appsv1.StatefulSetSpec{
			Replicas:    &replicasNum,
			ServiceName: params.ServiceName,
			Selector: &metav1.LabelSelector{
				MatchLabels: map[string]string{
					"tarantool.io/pod-template": params.PodTemplateName,
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
						"tarantool.io/cluster-id":          params.ClusterId,
						"tarantool.io/cluster-domain-name": "cluster.local",
						"tarantool.io/pod-template":        params.PodTemplateName,
						"tarantool.io/useVshardGroups":     "0",
						"environment":                      "dev",
					},
					Annotations: map[string]string{
						"tarantool.io/rolesToAssign": params.RolesToAssign,
					},
				},
				Spec: corev1.PodSpec{
					// TerminationGracePeriodSeconds: &terminationGracePeriod,
					// DNSConfig: &corev1.PodDNSConfig{
					// 	Options: []corev1.PodDNSConfigOption{
					// 		{
					// 			Name:  "ndots",
					// 			Value: &dnsOptionsValue,
					// 		},
					// 	},
					// },
					// SecurityContext: &corev1.PodSecurityContext{
					// 	FSGroup: &fsGroup,
					// },
					Containers: []corev1.Container{
						{
							Name:  params.ContainerName,
							Image: params.ContainerImage,
							VolumeMounts: []corev1.VolumeMount{
								{
									Name:      "www",
									MountPath: "/var/lib/tarantool",
								},
							},
							// SecurityContext: &corev1.SecurityContext{
							// 	Capabilities: &corev1.Capabilities{
							// 		Add: []corev1.Capability{
							// 			"SYS_ADMIN",
							// 		},
							// 	},
							// },
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
							Env: vars,
							// ReadinessProbe: &corev1.Probe{
							// 	Handler: corev1.Handler{
							// 		TCPSocket: &corev1.TCPSocketAction{
							// 			Port: intstr.IntOrString{
							// 				Type:   intstr.String,
							// 				StrVal: "http",
							// 			},
							// 		},
							// 	},
							// 	InitialDelaySeconds: int32(15),
							// 	PeriodSeconds:       int32(10),
							// },
						},
					},
				},
			},
		},
	}
}

// Create new corev1.Service to access instances of specific Tarantool role
func NewService(params ServiceParams) corev1.Service {
	return corev1.Service{
		ObjectMeta: metav1.ObjectMeta{
			Name:      params.Name,
			Namespace: params.Namespace,
			Labels: map[string]string{
				"tarantool.io/role": params.RoleName,
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
				"tarantool.io/role": params.RoleName,
			},
		},
	}
}
