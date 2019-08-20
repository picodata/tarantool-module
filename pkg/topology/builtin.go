package topology

import (
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"strings"

	corev1 "k8s.io/api/core/v1"
)

type ResponseError struct {
	Message string `json:"message"`
}
type JoinResponseData struct {
	JoinInstance bool `json:"join_instance"`
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

var (
	topologyIsDown      = errors.New("topology service is down")
	alreadyJoined       = errors.New("already joined")
	alreadyBootstrapped = errors.New("already bootstrapped")
)

func (s *BuiltInTopologyService) Join(pod *corev1.Pod) error {
	podIP := pod.Status.PodIP
	if len(podIP) == 0 {
		return errors.New("Pod.IP is not set yet, skip and wait")
	}
	advURI := fmt.Sprintf("%s:3301", podIP)
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
	req := fmt.Sprintf("mutation {join_instance: join_server(uri: \\\"%s\\\",instance_uuid: \\\"%s\\\",replicaset_uuid: \\\"%s\\\",roles: [\\\"%s\\\"],timeout: 10)}", advURI, instanceUUID, replicasetUUID, role)

	j := fmt.Sprintf("{\"query\": \"%s\"}", req)

	rawResp, err := http.Post(s.serviceHost, "application/json", strings.NewReader(j))
	if err != nil {
		return err
	}
	defer rawResp.Body.Close()

	resp := &JoinResponse{Errors: []*ResponseError{}, Data: &JoinResponseData{}}
	if err := json.NewDecoder(rawResp.Body).Decode(resp); err != nil {
		return err
	}

	if resp.Errors != nil && len(resp.Errors) > 0 {
		if strings.Contains(resp.Errors[0].Message, "already joined") {
			return alreadyJoined
		}
		if strings.Contains(resp.Errors[0].Message, "This instance isn't bootstrapped yet") {
			return topologyIsDown
		}

		return errors.New(resp.Errors[0].Message)
	}

	if resp.Data.JoinInstance == true {
		return nil
	}

	return errors.New("Undefined error")
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
		return errors.New("Shit happened")
	}

	return nil
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
