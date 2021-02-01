package controller

import (
    "context"
    "fmt"
    "net/http"
    "net/url"
    "os"
    "path/filepath"
    "strings"
    "testing"

    . "github.com/onsi/ginkgo"
    . "github.com/onsi/gomega"

    "github.com/onsi/gomega/types"
    "github.com/operator-framework/operator-sdk/pkg/log/zap"

    corev1 "k8s.io/api/core/v1"
    metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"

    "k8s.io/client-go/kubernetes/scheme"
    "k8s.io/client-go/rest"
    "k8s.io/client-go/tools/clientcmd"
    "k8s.io/client-go/tools/portforward"
    "k8s.io/client-go/transport/spdy"

    ctrl "sigs.k8s.io/controller-runtime"
    "sigs.k8s.io/controller-runtime/pkg/client"
    "sigs.k8s.io/controller-runtime/pkg/envtest"
    logf "sigs.k8s.io/controller-runtime/pkg/runtime/log"
    // +kubebuilder:scaffold:imports
    "github.com/tarantool/tarantool-operator/pkg/apis"
)

// These tests use Ginkgo (BDD-style Go testing framework). Refer to
// http://onsi.github.io/ginkgo/ to learn more about Ginkgo.

var cfg *rest.Config
var k8sClient client.Client
var testEnv *envtest.Environment
var stopCh chan struct{}

var (
    ClusterTestNamespace = "cluster-test-namespace"
)

func TestControllers(t *testing.T) {
    RegisterFailHandler(Fail)

    RunSpecsWithDefaultAndCustomReporters(t,
        "Controllers Suite",
        []Reporter{envtest.NewlineReporter{}})
}

var _ = BeforeSuite(func(done Done) {
    logf.SetLogger(zap.LoggerTo(GinkgoWriter))

    By("Bootstrapping test environment")
    testEnv = &envtest.Environment{
        CRDDirectoryPaths:  []string{filepath.Join("..", "..", "ci", "helm-chart", "crds")},
        UseExistingCluster: true,
    }

    var err error
    cfg, err = testEnv.Start()
    Expect(err).ToNot(HaveOccurred())
    Expect(cfg).ToNot(BeNil())

    err = apis.AddToScheme(scheme.Scheme)
    Expect(err).NotTo(HaveOccurred())

    // +kubebuilder:scaffold:scheme

    k8sClient, err = client.New(cfg, client.Options{Scheme: scheme.Scheme})
    Expect(err).ToNot(HaveOccurred())
    Expect(k8sClient).ToNot(BeNil())

    // create channel for stopping manager
    stopCh = make(chan struct{})

    mgr, err := ctrl.NewManager(cfg, ctrl.Options{})
    Expect(err).NotTo(HaveOccurred(), "failed to create manager")

    err = AddToManager(mgr)
    Expect(err).NotTo(HaveOccurred(), "failed to setup controller")

    go func() {
        err = mgr.Start(stopCh)
        Expect(err).NotTo(HaveOccurred(), "failed to start manager")
    }()

    By("Creating Cluster namespace")
    err = k8sClient.Create(context.TODO(), &corev1.Namespace{
        ObjectMeta: metav1.ObjectMeta{Name: ClusterTestNamespace},
    })
    Expect(err).NotTo(HaveOccurred(), "failed to create Cluster test namespace")

    close(done)
}, 60)

var _ = AfterSuite(func() {
    By("Deleting Cluster namespace")
    err := k8sClient.Delete(context.TODO(), &corev1.Namespace{
        ObjectMeta: metav1.ObjectMeta{Name: ClusterTestNamespace},
    })
    Expect(err).NotTo(HaveOccurred(), "failed to delete Cluster test namespace")

    close(stopCh)
    By("Tearing down the test environment")
    err = testEnv.Stop()
    Expect(err).ToNot(HaveOccurred())
})

func PortForwardToPod(pod *corev1.Pod, localPort int, podPort int, stopChan <-chan struct{}) {
    configPath := fmt.Sprintf("%s/.kube/config", os.Getenv("HOME")) // default Kind path
    if val := os.Getenv("KUBECONFIG"); val != "" {
        configPath = val
    }
    config, err := clientcmd.BuildConfigFromFlags("", configPath)
    Expect(err).ToNot(HaveOccurred())

    go func() {
        path := fmt.Sprintf("/api/v1/namespaces/%s/pods/%s/portforward", pod.Namespace, pod.Name)
        hostIP := strings.TrimLeft(config.Host, "htps:/")

        transport, upgrader, err := spdy.RoundTripperFor(config)
        Expect(err).ToNot(HaveOccurred())

        dialer := spdy.NewDialer(
            upgrader,
            &http.Client{Transport: transport},
            http.MethodPost,
            &url.URL{Scheme: "https", Path: path, Host: hostIP},
        )

        fw, err := portforward.New(
            dialer,
            []string{fmt.Sprintf("%d:%d", localPort, podPort)},
            stopChan,
            make(chan struct{}),
            GinkgoWriter,
            GinkgoWriter,
        )
        Expect(err).ToNot(HaveOccurred())
        Expect(fw.ForwardPorts()).ToNot(HaveOccurred())
    }()
}

type rolesMatcher struct {
    expected interface{}
}

func (matcher *rolesMatcher) Match(actual interface{}) (success bool, err error) {
    actualRoles, ok := actual.([]string)
    if !ok {
        return false, fmt.Errorf("Actual value must be of type []string")
    }

    expectRoles, ok := matcher.expected.([]string)
    if !ok {
        return false, fmt.Errorf("Excepted value must be of type []string")
    }

    isSubset := func(X, Y []string) bool {
        for _, x := range X {
            match := false
            for _, y := range Y {
                if x == y {
                    match = true
                    break
                }
            }
            if !match {
                return false
            }
        }
        return true
    }
    return isSubset(actualRoles, expectRoles) && isSubset(expectRoles, actualRoles), nil
}
func (matcher *rolesMatcher) FailureMessage(actual interface{}) (message string) {
    return fmt.Sprintf("\n\t%#v is expected to be equal to \t%#v", actual.([]string), matcher.expected.([]string))
}

func (matcher *rolesMatcher) NegatedFailureMessage(actual interface{}) (message string) {
    return fmt.Sprintf("\n\t%#v is expected to be unequal to \t%#v", actual.([]string), matcher.expected.([]string))
}

func RolesMatcherObject(expected interface{}) types.GomegaMatcher {
    return &rolesMatcher{
        expected: expected,
    }
}
