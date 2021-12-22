package tarantool

import (
	corev1 "k8s.io/api/core/v1"
	"k8s.io/apimachinery/pkg/labels"
	"k8s.io/apimachinery/pkg/selection"
)

const (
	instanceJoined    = "joined"
	instanceExpelling = "expelling"
)

// IsJoined .
func IsJoined(p *corev1.Pod) bool {
	podLabels := p.GetLabels()
	if podLabels == nil {
		return false
	}
	v, ok := podLabels["tarantool.io/instance-state"]
	if !ok {
		return false
	}
	if v != instanceJoined {
		return false
	}

	return true
}

// MarkJoined .
func MarkJoined(p *corev1.Pod) {
	podLabels := p.GetLabels()
	if podLabels == nil {
		podLabels = make(map[string]string)
	}
	podLabels["tarantool.io/instance-state"] = instanceJoined
	p.SetLabels(podLabels)
}

// JoinedSelector .
func JoinedSelector() (labels.Selector, error) {
	s := labels.NewSelector()
	r, err := labels.NewRequirement("tarantool.io/instance-state", selection.Equals, []string{instanceJoined})
	if err != nil {
		return nil, err
	}
	s.Add(*r)

	return s, nil
}

// IsExpelling .
func IsExpelling(p *corev1.Pod) bool {
	podLabels := p.GetLabels()
	if podLabels == nil {
		return false
	}
	v, ok := podLabels["tarantool.io/instance-state"]
	if !ok {
		return false
	}
	if v != instanceExpelling {
		return false
	}

	return true
}

// MarkExpelling .
func MarkExpelling(p *corev1.Pod) {
	podLabels := p.GetLabels()
	if podLabels == nil {
		podLabels = make(map[string]string)
	}
	podLabels["tarantool.io/instance-state"] = instanceExpelling
	p.SetLabels(podLabels)
}

// ExpellingSelector .
func ExpellingSelector() (labels.Selector, error) {
	s := labels.NewSelector()
	r, err := labels.NewRequirement("tarantool.io/instance-state", selection.Equals, []string{instanceExpelling})
	if err != nil {
		return nil, err
	}
	s.Add(*r)

	return s, nil
}
