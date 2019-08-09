package v1alpha1

import (
	appsv1 "k8s.io/api/apps/v1"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
)

// EDIT THIS FILE!  THIS IS SCAFFOLDING FOR YOU TO OWN!
// NOTE: json tags are required.  Any new fields you add must have json tags for the fields to be serialized.

// TarantoolClusterSpec defines the desired state of TarantoolCluster
// +k8s:openapi-gen=true
type TarantoolClusterSpec struct {
	// INSERT ADDITIONAL SPEC FIELDS - desired state of cluster
	// Important: Run "operator-sdk generate k8s" to regenerate code after modifying this file
	// Add custom validation using kubebuilder tags: https://book-v1.book.kubebuilder.io/beyond_basics/generating_crd.html
	ReplicasetTemplate appsv1.StatefulSetSpec `json:"template,omitempty"`
}

// TarantoolClusterStatus defines the observed state of TarantoolCluster
// +k8s:openapi-gen=true
type TarantoolClusterStatus struct {
	// INSERT ADDITIONAL STATUS FIELD - define observed state of cluster
	// Important: Run "operator-sdk generate k8s" to regenerate code after modifying this file
	// Add custom validation using kubebuilder tags: https://book-v1.book.kubebuilder.io/beyond_basics/generating_crd.html
}

// +k8s:deepcopy-gen:interfaces=k8s.io/apimachinery/pkg/runtime.Object

// TarantoolCluster is the Schema for the tarantoolclusters API
// +k8s:openapi-gen=true
// +kubebuilder:subresource:status
type TarantoolCluster struct {
	metav1.TypeMeta   `json:",inline"`
	metav1.ObjectMeta `json:"metadata,omitempty"`

	Spec   TarantoolClusterSpec   `json:"spec,omitempty"`
	Status TarantoolClusterStatus `json:"status,omitempty"`
}

// +k8s:deepcopy-gen:interfaces=k8s.io/apimachinery/pkg/runtime.Object

// TarantoolClusterList contains a list of TarantoolCluster
type TarantoolClusterList struct {
	metav1.TypeMeta `json:",inline"`
	metav1.ListMeta `json:"metadata,omitempty"`
	Items           []TarantoolCluster `json:"items"`
}

func init() {
	SchemeBuilder.Register(&TarantoolCluster{}, &TarantoolClusterList{})
}
