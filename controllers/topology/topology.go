package topology

import (
	corev1 "k8s.io/api/core/v1"
)

// TopologyService .
type TopologyService interface {
	Join(p *corev1.Pod) error
	Expel(p *corev1.Pod) error
}
