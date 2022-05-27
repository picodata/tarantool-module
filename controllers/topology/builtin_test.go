package topology

import (
	"encoding/json"
	"io"
	"io/ioutil"
	"net/http"
	"net/http/httptest"
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

type FailoverVariables struct {
	Mode string `json:"mode"`
}

type FailoverQuery struct {
	Query     string            `json:"query"`
	Variables FailoverVariables `json:"variables"`
}

var setFailoverGQL = `mutation setFailoverMode($mode: String) {
	cluster {
		failover_params(mode: $mode) {
		  mode
		}
	}
}`

func TestSetFailover(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		b, err := ioutil.ReadAll(r.Body)
		if err != nil {
			t.Fatalf("%s", err)
		}
		query := FailoverQuery{}
		if err = json.Unmarshal(b, &query); err != nil {
			t.Fatalf("Wrong qeury: %s", err)
		}

		if query.Query != setFailoverGQL {
			t.Fatalf("Wrong query: %s", query.Query)
		}

		if query.Variables.Mode != "eventual" {
			t.Fatalf("Wrong failover type: %s", query.Variables.Mode)
		}

		_, _ = io.WriteString(w, `{
			"data": {
			  "cluster": {
				"failover_params": {
				  "mode": "eventual"
				}
			  }
			}
		}`)
	}))

	defer srv.Close()

	topology := BuiltInTopologyService{
		serviceHost: srv.URL,
		clusterID:   "uuid",
	}

	err := topology.SetFailover(true)
	if err != nil {
		t.Fatalf("%s", err)
	}
}

var getFailoverGQL = `query {
	cluster {
		failover_params {
			mode
		}
	}
}`

func TestGetFailover(t *testing.T) {
	var failoverMode string
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		b, err := ioutil.ReadAll(r.Body)
		if err != nil {
			t.Fatalf("%s", err)
		}
		query := FailoverQuery{}
		if err = json.Unmarshal(b, &query); err != nil {
			t.Fatalf("Wrong qeury: %s", err)
		}

		if query.Query != getFailoverGQL {
			t.Fatalf("Wrong query: %s", query.Query)
		}

		_, _ = io.WriteString(w, failoverMode)
	}))

	defer srv.Close()

	topology := BuiltInTopologyService{
		serviceHost: srv.URL,
		clusterID:   "uuid",
	}

	failoverMode = `{
		"data": {
		  "cluster": {
			"failover_params": {
			  "mode": "eventual"
			}
		  }
		}
	}`

	enabled, err := topology.GetFailover()
	if err != nil {
		t.Fatalf("%s", err)
	}

	if !enabled {
		t.Fatal("Failover should be enabled")
	}

	failoverMode = `{
		"data": {
		  "cluster": {
			"failover_params": {
			  "mode": "stateful"
			}
		  }
		}
	}`

	enabled, err = topology.GetFailover()
	if err != nil {
		t.Fatalf("%s", err)
	}

	if !enabled {
		t.Fatal("Failover should be enabled")
	}

	failoverMode = `{
		"data": {
		  "cluster": {
			"failover_params": {
			  "mode": "disabled"
			}
		  }
		}
	}`

	enabled, err = topology.GetFailover()
	if err != nil {
		t.Fatalf("%s", err)
	}

	if enabled {
		t.Fatal("Failover should be disabled")
	}

	failoverMode = `{
		"data": {
		  "cluster": {
		  }
		}
	}`

	_, err = topology.GetFailover()
	if err == nil {
		t.Fatal("Wrong answer format, but error wasn't thrown")
	}
}
