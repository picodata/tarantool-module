package topology

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"strings"
	"time"

	"github.com/machinebox/graphql"
	corev1 "k8s.io/api/core/v1"
)

type ResponseError struct {
	Message string `json:"message"`
}
type JoinResponseData struct {
	JoinInstance bool `json:"joinInstanceResponse"`
}
type JoinResponse struct {
	Errors []*ResponseError  `json:"errors,omitempty"`
	Data   *JoinResponseData `json:"data,omitempty"`
}

type ExpelResponseData struct {
	ExpelInstance bool `json:"expel_instance"`
}
type ExpelResponse struct {
	Errors []*ResponseError   `json:"errors,omitempty"`
	Data   *ExpelResponseData `json:"data,omitempty"`
}

type BootstrapVshardData struct {
	BootstrapVshard bool `json:"bootstrapVshardResponse"`
}
type BootstrapVshardResponse struct {
	Data   *BootstrapVshardData `json:"data,omitempty"`
	Errors []*ResponseError     `json:"errors,omitempty"`
}

type BuiltInTopologyService struct {
	serviceHost string
}

type EditReplicasetResponse struct {
	Response bool `json:"editReplicasetResponse"`
}

var (
	topologyIsDown      = errors.New("topology service is down")
	alreadyJoined       = errors.New("already joined")
	alreadyBootstrapped = errors.New("already bootstrapped")
)

var join_mutation = `mutation do_join_server($uri: String!, $instance_uuid: String!, $replicaset_uuid: String!, $roles: [String!]) {
	joinInstanceResponse: join_server(uri: $uri, instance_uuid: $instance_uuid, replicaset_uuid: $replicaset_uuid, roles: $roles, timeout: 10)
}`
var edit_rs_mutation = `mutation editReplicaset($uuid: String!, $weight: Float) {
	editReplicasetResponse: edit_replicaset(uuid: $uuid, weight: $weight)
}`

func (s *BuiltInTopologyService) Join(pod *corev1.Pod) error {
	advURI := fmt.Sprintf("%s.examples-kv-cluster:3301", pod.GetObjectMeta().GetName())
	replicasetUUID, ok := pod.GetLabels()["tarantool.io/replicaset-uuid"]
	if !ok {
		return errors.New("replicaset uuid empty")
	}

	instanceUUID, ok := pod.GetLabels()["tarantool.io/instance-uuid"]
	if !ok {
		return errors.New("instance uuid empty")
	}

	role, ok := pod.GetLabels()["tarantool.io/role"]
	if !ok {
		return errors.New("role undefined")
	}

	client := graphql.NewClient(s.serviceHost, graphql.WithHTTPClient(&http.Client{Timeout: time.Duration(time.Second * 5)}))
	req := graphql.NewRequest(join_mutation)

	req.Var("uri", advURI)
	req.Var("instance_uuid", instanceUUID)
	req.Var("replicaset_uuid", replicasetUUID)
	req.Var("roles", []string{role})

	resp := &JoinResponseData{}
	if err := client.Run(context.TODO(), req, resp); err != nil {
		if strings.Contains(err.Error(), "already joined") {
			return alreadyJoined
		}
		if strings.Contains(err.Error(), "This instance isn't bootstrapped yet") {
			return topologyIsDown
		}

		return err
	}

	if resp.JoinInstance == true {
		return nil
	}

	return errors.New("something really bad happened")
}

func (s *BuiltInTopologyService) Expel(pod *corev1.Pod) error {
	req := fmt.Sprintf("mutation {expel_instance:expel_server(uuid:\\\"%s\\\")}", pod.GetAnnotations()["tarantool.io/instance_uuid"])
	j := fmt.Sprintf("{\"query\": \"%s\"}", req)
	rawResp, err := http.Post(s.serviceHost, "application/json", strings.NewReader(j))
	if err != nil {
		return err
	}
	defer rawResp.Body.Close()

	resp := &ExpelResponse{Errors: []*ResponseError{}, Data: &ExpelResponseData{}}
	if err := json.NewDecoder(rawResp.Body).Decode(resp); err != nil {
		return err
	}

	if resp.Data.ExpelInstance == false && (resp.Errors == nil || len(resp.Errors) == 0) {
		return errors.New("something really bad happened")
	}

	return nil
}

func (s *BuiltInTopologyService) SetWeight(replicasetUUID string) error {
	client := graphql.NewClient(s.serviceHost, graphql.WithHTTPClient(&http.Client{Timeout: time.Duration(time.Second * 5)}))
	req := graphql.NewRequest(edit_rs_mutation)

	req.Var("uuid", replicasetUUID)
	req.Var("weight", 1)

	resp := &EditReplicasetResponse{}
	if err := client.Run(context.TODO(), req, resp); err != nil {
		return err
	}
	if resp.Response == true {
		return nil
	}

	return errors.New("something really bad happened")
}

func (s *BuiltInTopologyService) BootstrapVshard() error {
	req := fmt.Sprint("mutation bootstrap {bootstrapVshardResponse: bootstrap_vshard}")
	j := fmt.Sprintf("{\"query\": \"%s\"}", req)
	rawResp, err := http.Post(s.serviceHost, "application/json", strings.NewReader(j))
	if err != nil {
		return err
	}
	defer rawResp.Body.Close()

	resp := &BootstrapVshardResponse{Data: &BootstrapVshardData{}}
	if err := json.NewDecoder(rawResp.Body).Decode(resp); err != nil {
		return err
	}

	if resp.Data.BootstrapVshard {
		return nil
	}
	if resp.Errors != nil && len(resp.Errors) > 0 {
		if strings.Contains(resp.Errors[0].Message, "already bootstrapped") {
			return alreadyBootstrapped
		}

		return errors.New(resp.Errors[0].Message)
	}

	return errors.New("unknown error")
}

func IsTopologyDown(err error) bool {
	return err == topologyIsDown
}

func IsAlreadyJoined(err error) bool {
	return err == alreadyJoined
}

func IsAlreadyBootstrapped(err error) bool {
	return err == alreadyBootstrapped
}

type Option func(s *BuiltInTopologyService)

func WithTopologyEndpoint(url string) Option {
	return func(s *BuiltInTopologyService) {
		s.serviceHost = url
	}
}

func NewBuiltInTopologyService(opts ...Option) *BuiltInTopologyService {
	s := &BuiltInTopologyService{}
	for _, opt := range opts {
		opt(s)
	}

	return s
}
