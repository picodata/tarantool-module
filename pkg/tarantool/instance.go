package tarantool

import (
	corev1 "k8s.io/api/core/v1"
	"k8s.io/apimachinery/pkg/labels"
	"k8s.io/apimachinery/pkg/selection"
)

const (
	INSTANCE_JOINED    = "joined"
	INSTANCE_EXPELLING = "expelling"
)

func IsJoined(p *corev1.Pod) bool {
	podLabels := p.GetLabels()
	if podLabels == nil {
		return false
	}
	v, ok := podLabels["tarantool.io/instance-state"]
	if !ok {
		return false
	}
	if v != INSTANCE_JOINED {
		return false
	}

	return true
}

func MarkJoined(p *corev1.Pod) {
	podLabels := p.GetLabels()
	if podLabels == nil {
		podLabels = make(map[string]string)
	}
	podLabels["tarantool.io/instance-state"] = INSTANCE_JOINED
	p.SetLabels(podLabels)
}

func JoinedSelector() (labels.Selector, error) {
	s := labels.NewSelector()
	r, err := labels.NewRequirement("tarantool.io/instance-state", selection.Equals, []string{INSTANCE_JOINED})
	if err != nil {
		return nil, err
	}
	s.Add(*r)

	return s, nil
}

func IsExpelling(p *corev1.Pod) bool {
	podLabels := p.GetLabels()
	if podLabels == nil {
		return false
	}
	v, ok := podLabels["tarantool.io/instance-state"]
	if !ok {
		return false
	}
	if v != INSTANCE_EXPELLING {
		return false
	}

	return true
}

func MarkExpelling(p *corev1.Pod) {
	podLabels := p.GetLabels()
	if podLabels == nil {
		podLabels = make(map[string]string)
	}
	podLabels["tarantool.io/instance-state"] = INSTANCE_EXPELLING
	p.SetLabels(podLabels)
}

func ExpellingSelector() (labels.Selector, error) {
	s := labels.NewSelector()
	r, err := labels.NewRequirement("tarantool.io/instance-state", selection.Equals, []string{INSTANCE_EXPELLING})
	if err != nil {
		return nil, err
	}
	s.Add(*r)

	return s, nil
}
