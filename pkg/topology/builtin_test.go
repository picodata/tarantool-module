package topology

import (
	"strings"
	"testing"

	corev1 "k8s.io/api/core/v1"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
)

func Contains(a []string, x string) bool {
	for _, n := range a {
		if x == n {
			return true
		}
	}
	return false
}

type errorTestCase struct {
	pod         *corev1.Pod
	expectedErr string
}

func TestGetRoles_ErrorCases(t *testing.T) {
	cases := []errorTestCase{
		{
			pod:         &corev1.Pod{},
			expectedErr: "role undefined",
		},
		{
			pod: &corev1.Pod{
				ObjectMeta: metav1.ObjectMeta{
					Labels: map[string]string{},
				},
			},
			expectedErr: "role undefined",
		},
		{
			pod: &corev1.Pod{
				ObjectMeta: metav1.ObjectMeta{
					Labels: map[string]string{
						"tarantool.io/someLabel": "some value",
					},
				},
			},
			expectedErr: "role undefined",
		},
		{
			pod: &corev1.Pod{
				ObjectMeta: metav1.ObjectMeta{
					Annotations: map[string]string{},
					Labels:      map[string]string{},
				},
			},
			expectedErr: "role undefined",
		},
	}
	for i, c := range cases {
		roles, err := GetRoles(c.pod)

		if roles != nil {
			t.Fatalf("%d: roles must be nil", i)
		}

		if strings.Contains(err.Error(), c.expectedErr) == false {
			t.Fatalf("%d: expected error %s, got %s", i, c.expectedErr, err.Error())
		}
	}
}

type parseRolesTestCase struct {
	pod           *corev1.Pod
	expectedRoles []string
}

func TestGetRoles_ParsesRolesFromLabels(t *testing.T) {
	cases := []parseRolesTestCase{
		{
			pod: &corev1.Pod{
				ObjectMeta: metav1.ObjectMeta{
					Labels: map[string]string{
						"tarantool.io/rolesToAssign": "router",
					},
				},
			},
			expectedRoles: []string{"router"},
		},
		{
			pod: &corev1.Pod{
				ObjectMeta: metav1.ObjectMeta{
					Labels: map[string]string{
						"tarantool.io/rolesToAssign": "router.storage",
					},
				},
			},
			expectedRoles: []string{"router", "storage"},
		},
	}

	for i, c := range cases {
		roles, _ := GetRoles(c.pod)
		if len(roles) != len(c.expectedRoles) {
			t.Fatalf("%d: expected %d roles, got %d", i, len(c.expectedRoles), len(roles))
		}

		for _, v := range c.expectedRoles {
			if Contains(roles, v) == false {
				t.Fatalf("%d: roles must contain %s", i, v)
			}
		}
	}
}

func TestGetRoles_ParseRolesFromAnnotations(t *testing.T) {
	cases := []parseRolesTestCase{
		{
			pod: &corev1.Pod{
				ObjectMeta: metav1.ObjectMeta{
					Annotations: map[string]string{
						"tarantool.io/rolesToAssign": `"router"`,
					},
				},
			},
			expectedRoles: []string{"router"},
		},
		{
			pod: &corev1.Pod{
				ObjectMeta: metav1.ObjectMeta{
					Annotations: map[string]string{
						"tarantool.io/rolesToAssign": `["router", "vshard.storage"]`,
					},
				},
			},
			expectedRoles: []string{"router", "vshard.storage"},
		},
	}

	for i, c := range cases {
		roles, _ := GetRoles(c.pod)
		if len(roles) != len(c.expectedRoles) {
			t.Fatalf("%d: expected %d roles, got %d", i, len(c.expectedRoles), len(roles))
		}

		for _, v := range c.expectedRoles {
			if Contains(roles, v) == false {
				t.Fatalf("%d: roles must contain %s", i, v)
			}
		}
	}
}
