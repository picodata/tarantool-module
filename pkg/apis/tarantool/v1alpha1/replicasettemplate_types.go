package v1alpha1

import (
	appsv1 "k8s.io/api/apps/v1"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
)

// EDIT THIS FILE!  THIS IS SCAFFOLDING FOR YOU TO OWN!
// NOTE: json tags are required.  Any new fields you add must have json tags for the fields to be serialized.

// ReplicasetTemplateSpec defines the desired state of ReplicasetTemplate
// +k8s:openapi-gen=true
type ReplicasetTemplateSpec struct {
	// INSERT ADDITIONAL SPEC FIELDS - desired state of cluster
	// Important: Run "operator-sdk generate k8s" to regenerate code after modifying this file
	// Add custom validation using kubebuilder tags: https://book-v1.book.kubebuilder.io/beyond_basics/generating_crd.html
}

// ReplicasetTemplateStatus defines the observed state of ReplicasetTemplate
// +k8s:openapi-gen=true
type ReplicasetTemplateStatus struct {
	// INSERT ADDITIONAL STATUS FIELD - define observed state of cluster
	// Important: Run "operator-sdk generate k8s" to regenerate code after modifying this file
	// Add custom validation using kubebuilder tags: https://book-v1.book.kubebuilder.io/beyond_basics/generating_crd.html
}

// +k8s:deepcopy-gen:interfaces=k8s.io/apimachinery/pkg/runtime.Object

// ReplicasetTemplate is the Schema for the replicasettemplates API
// +k8s:openapi-gen=true
// +kubebuilder:subresource:status
type ReplicasetTemplate struct {
	metav1.TypeMeta   `json:",inline"`
	metav1.ObjectMeta `json:"metadata,omitempty"`

	Spec   *appsv1.StatefulSetSpec  `json:"spec,omitempty"`
	Status ReplicasetTemplateStatus `json:"status,omitempty"`
}

// +k8s:deepcopy-gen:interfaces=k8s.io/apimachinery/pkg/runtime.Object

// ReplicasetTemplateList contains a list of ReplicasetTemplate
type ReplicasetTemplateList struct {
	metav1.TypeMeta `json:",inline"`
	metav1.ListMeta `json:"metadata,omitempty"`
	Items           []ReplicasetTemplate `json:"items"`
}

func init() {
	SchemeBuilder.Register(&ReplicasetTemplate{}, &ReplicasetTemplateList{})
}
