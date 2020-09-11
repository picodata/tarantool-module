package topology

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"strconv"
	"strings"
	"time"

	"github.com/machinebox/graphql"
	corev1 "k8s.io/api/core/v1"
	logf "sigs.k8s.io/controller-runtime/pkg/runtime/log"
)

// ResponseError .
type ResponseError struct {
	Message string `json:"message"`
}

// JoinResponseData .
type JoinResponseData struct {
	JoinInstance bool `json:"joinInstanceResponse"`
}

// JoinResponse .
type JoinResponse struct {
	Errors []*ResponseError  `json:"errors,omitempty"`
	Data   *JoinResponseData `json:"data,omitempty"`
}

// ExpelResponseData .
type ExpelResponseData struct {
	ExpelInstance bool `json:"expel_instance"`
}

// ExpelResponse .
type ExpelResponse struct {
	Errors []*ResponseError   `json:"errors,omitempty"`
	Data   *ExpelResponseData `json:"data,omitempty"`
}

// BootstrapVshardData .
type BootstrapVshardData struct {
	BootstrapVshard bool `json:"bootstrapVshardResponse"`
}

// BootstrapVshardResponse .
type BootstrapVshardResponse struct {
	Data   *BootstrapVshardData `json:"data,omitempty"`
	Errors []*ResponseError     `json:"errors,omitempty"`
}

// FailoverData Structure of data for changing failover status
type FailoverData struct {
	failover *FailoverParams `json:"failover_params"`
}

// FailoverParams returns the mode of failover which has been enabled
type FailoverParams struct {
	mode string `json:"mode"`
}

// FailoverResponse type struct for returning on failovers
type FailoverResponse struct {
	Data   *FailoverData
	Errors []*ResponseError
}

// BuiltInTopologyService .
type BuiltInTopologyService struct {
	serviceHost string
	clusterID   string
}

// EditReplicasetResponse .
type EditReplicasetResponse struct {
	Response bool `json:"editReplicasetResponse"`
}

// GetServerStatResponse .
type GetServerStatResponse struct {
	Data   *ServerStatData  `json:"data"`
	Errors []*ResponseError `json:"errors,omitempty"`
}

// ServerStatData .
type ServerStatData struct {
	Stats []*ServerStat `json:"serverStat"`
}

// ServerStat .
type ServerStat struct {
	Statistics Statistics `json:"statistics"`
	UUID       string     `json:"uuid"`
	URI        string     `json:"uri"`
}

// Statistics .
type Statistics struct {
	ItemsUsedRatio string `json:"items_used_ratio"`
	ArenaUsedRatio string `json:"arena_used_ratio"`
	QuotaSize      int    `json:"quotaSize"`
	ArenaUsed      int    `json:"arenaUsed"`
	QuotaUsedRatio string `json:"quota_used_ratio"`
	BucketsCount   int    `json:"bucketsCount"`
}

var log = logf.Log.WithName("topology")

var (
	errTopologyIsDown      = errors.New("topology service is down")
	errAlreadyJoined       = errors.New("already joined")
	errAlreadyBootstrapped = errors.New("already bootstrapped")
)

var joinMutation = `mutation
	do_join_server(
		$uri: String!,
		$instance_uuid: String!,
		$replicaset_uuid: String!,
		$roles: [String!],
		$vshard_group: String!
	) {
	joinInstanceResponse: join_server(
		uri: $uri,
		instance_uuid: $instance_uuid,
		replicaset_uuid: $replicaset_uuid,
		roles: $roles,
		timeout: 10,
		vshard_group: $vshard_group
	)
}`
var editRsMutation = `mutation editReplicaset($uuid: String!, $weight: Float) {
	editReplicasetResponse: edit_replicaset(uuid: $uuid, weight: $weight)
}`

var getServerStatQuery = `query serverList {
	serverStat: servers {
		uuid
		uri
		statistics {
			quotaSize: quota_size
			arenaUsed: arena_used
			bucketsCount: vshard_buckets_count
			quota_used_ratio
			arena_used_ratio
			items_used_ratio
		}
	}
}`

// GetRoles comment
func GetRoles(pod *corev1.Pod) ([]string, error) {
	thisPodLabels := pod.GetLabels()
	thisPodAnnotations := pod.GetAnnotations()

	rolesFromAnnotations, ok := thisPodAnnotations["tarantool.io/rolesToAssign"]
	if !ok {
		rolesFromLabels, ok := thisPodLabels["tarantool.io/rolesToAssign"]
		if !ok {
			return nil, errors.New("role undefined")
		}

		roles := strings.Split(rolesFromLabels, ".")
		log.Info("roles", "roles", roles)

		return roles, nil
	}

	var singleRole string
	var roleArray []string

	err := json.Unmarshal([]byte(rolesFromAnnotations), &singleRole)
	if err == nil {
		log.Info("roles", "roles", singleRole)
		return []string{singleRole}, nil
	}

	err = json.Unmarshal([]byte(rolesFromAnnotations), &roleArray)
	if err == nil {
		log.Info("roles", "roles", roleArray)
		return roleArray, nil
	}

	return nil, errors.New("failed to parse roles from annotations")
}

// Join comment
func (s *BuiltInTopologyService) Join(pod *corev1.Pod) error {

	advURI := fmt.Sprintf("%s.%s.%s.svc.cluster.local:3301", pod.GetObjectMeta().GetName(), s.clusterID, pod.GetObjectMeta().GetNamespace())

	thisPodLabels := pod.GetLabels()

	replicasetUUID, ok := thisPodLabels["tarantool.io/replicaset-uuid"]
	if !ok {
		return errors.New("replicaset uuid empty")
	}

	log.Info("payload", "advURI", advURI, "replicasetUUID", replicasetUUID)

	instanceUUID, ok := thisPodLabels["tarantool.io/instance-uuid"]
	if !ok {
		return errors.New("instance uuid empty")
	}

	roles, err := GetRoles(pod)
	if err != nil {
		return err
	}
	log.Info("roles", "roles", roles)

	vshardGroup := "default"
	useVshardGroups, ok := thisPodLabels["tarantool.io/useVshardGroups"]
	if !ok {
		return errors.New("failed to get label tarantool.io/useVshardGroups")
	}

	if useVshardGroups == "1" {
		vshardGroup, ok = thisPodLabels["tarantool.io/vshardGroupName"]
		if !ok {
			return errors.New("vshard_group undefined")
		}
	}

	client := graphql.NewClient(s.serviceHost, graphql.WithHTTPClient(&http.Client{Timeout: time.Duration(time.Second * 5)}))
	req := graphql.NewRequest(joinMutation)

	req.Var("uri", advURI)
	req.Var("instance_uuid", instanceUUID)
	req.Var("replicaset_uuid", replicasetUUID)
	req.Var("roles", roles)
	req.Var("vshard_group", vshardGroup)

	resp := &JoinResponseData{}
	if err := client.Run(context.TODO(), req, resp); err != nil {

		if strings.Contains(err.Error(), "already joined") {
			return errAlreadyJoined
		}
		if strings.Contains(err.Error(), "This instance isn't bootstrapped yet") {
			return errTopologyIsDown
		}

		return err
	}

	if resp.JoinInstance == true {
		return nil
	}

	return errors.New("something really bad happened")
}

// SetFailover enables cluster failover
func (s *BuiltInTopologyService) SetFailover(enabled bool) error {
	client := graphql.NewClient(s.serviceHost, graphql.WithHTTPClient(&http.Client{Timeout: time.Duration(time.Second * 5)}))
	req := graphql.NewRequest(`mutation changeFailover($enabled: Boolean!) { cluster { failover(enabled: $enabled) }}`)

	req.Var("enabled", enabled)

	resp := &FailoverData{}
	if err := client.Run(context.TODO(), req, resp); err != nil {
		log.Error(err, "failoverError")
		return errors.New("failed to enable cluster failover")
	}

	return nil
}

// Expel removes an instance from the replicaset
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

// SetWeight sets weight of a replicaset
func (s *BuiltInTopologyService) SetWeight(replicasetUUID string, replicaWeight string) error {
	client := graphql.NewClient(s.serviceHost, graphql.WithHTTPClient(&http.Client{Timeout: time.Duration(time.Second * 5)}))
	req := graphql.NewRequest(editRsMutation)

	reqLogger := log.WithValues("namespace", "topology.builtin")

	weightParam, err := strconv.ParseUint(replicaWeight, 10, 32)
	if err != nil {
		return err
	}

	reqLogger.Info("setting cluster weight", "uuid", replicasetUUID, "weight", replicaWeight)

	req.Var("uuid", replicasetUUID)
	req.Var("weight", weightParam)

	resp := &EditReplicasetResponse{}
	if err := client.Run(context.TODO(), req, resp); err != nil {
		return err
	}

	if resp.Response == true {
		return nil
	}

	return errors.New("something really bad happened")
}

// GetServerStat Fetch the replicaset as reported by cartridge
func (s *BuiltInTopologyService) GetServerStat() (ServerStatData, error) {
	client := graphql.NewClient(s.serviceHost, graphql.WithHTTPClient(&http.Client{Timeout: time.Duration(time.Second * 5)}))
	req := graphql.NewRequest(getServerStatQuery)

	reqLogger := log.WithValues("function", "GetServerStat")

	reqLogger.Info("fetching server stats")

	resp := ServerStatData{}
	if err := client.Run(context.TODO(), req, &resp); err != nil {
		return resp, err
	}

	return resp, nil
}

// BootstrapVshard enable the vshard service on the cluster
func (s *BuiltInTopologyService) BootstrapVshard() error {
	reqLogger := log.WithValues("namespace", "topology.builtin")

	reqLogger.Info("Bootstrapping vshard")

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
			return errAlreadyBootstrapped
		}

		return errors.New(resp.Errors[0].Message)
	}

	return errors.New("unknown error")
}

// IsTopologyDown .
func IsTopologyDown(err error) bool {
	return err == errTopologyIsDown
}

// IsAlreadyJoined .
func IsAlreadyJoined(err error) bool {
	return err == errAlreadyJoined
}

// IsAlreadyBootstrapped .
func IsAlreadyBootstrapped(err error) bool {
	return err == errAlreadyBootstrapped
}

// Option .
type Option func(s *BuiltInTopologyService)

// WithTopologyEndpoint .
func WithTopologyEndpoint(url string) Option {
	return func(s *BuiltInTopologyService) {
		s.serviceHost = url
	}
}

// WithClusterID .
func WithClusterID(id string) Option {
	return func(s *BuiltInTopologyService) {
		s.clusterID = id
	}
}

// NewBuiltInTopologyService .
func NewBuiltInTopologyService(opts ...Option) *BuiltInTopologyService {
	s := &BuiltInTopologyService{}
	for _, opt := range opts {
		opt(s)
	}

	return s
}
