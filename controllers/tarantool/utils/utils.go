package utils

import (
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
)

// SetComponent .
func SetComponent(o metav1.Object, componentName string) error {
	labels := o.GetLabels()
	if labels == nil {
		labels = make(map[string]string)
	}
	labels["app.kubernetes.io/component"] = componentName
	o.SetLabels(labels)

	return nil
}

// SetPartOf .
func SetPartOf(o metav1.Object, appName string) error {
	labels := o.GetLabels()
	if labels == nil {
		labels = make(map[string]string)
	}
	labels["app.kubernetes.io/part-of"] = appName
	o.SetLabels(labels)

	return nil
}

// SetTarantoolClusterID .
func SetTarantoolClusterID(o metav1.Object, clusteID string) error {
	labels := o.GetLabels()
	if labels == nil {
		labels = make(map[string]string)
	}
	labels["tarantool.io/cluster-id"] = clusteID
	o.SetLabels(labels)

	return nil
}
