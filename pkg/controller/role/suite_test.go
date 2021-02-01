package role

import (
    // "context"
    "math/rand"
    "path/filepath"
    "testing"
    "time"

    . "github.com/onsi/ginkgo"
    . "github.com/onsi/gomega"
    "github.com/operator-framework/operator-sdk/pkg/log/zap"

    // corev1 "k8s.io/api/core/v1"
    // metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"

    "k8s.io/client-go/kubernetes/scheme"
    "k8s.io/client-go/rest"

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
    TestNamespace = "test-namespace"
)

func TestRoleController(t *testing.T) {
    RegisterFailHandler(Fail)

    RunSpecsWithDefaultAndCustomReporters(t,
        "Role Controller Suite",
        []Reporter{envtest.NewlineReporter{}})
}

var _ = BeforeSuite(func(done Done) {
    logf.SetLogger(zap.LoggerTo(GinkgoWriter))

    By("Bootstrapping test environment")
    testEnv = &envtest.Environment{
        CRDDirectoryPaths:  []string{filepath.Join("..", "..", "..", "ci", "helm-chart", "crds")},
        UseExistingCluster: false,
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

    err = Add(mgr)
    Expect(err).NotTo(HaveOccurred(), "failed to setup controller")

    go func() {
        err = mgr.Start(stopCh)
        Expect(err).NotTo(HaveOccurred(), "failed to start manager")
    }()

    close(done)
}, 60)

var _ = AfterSuite(func() {
    close(stopCh)
    By("Tearing down the test environment")
    err := testEnv.Stop()
    Expect(err).ToNot(HaveOccurred())
})

func init() {
    rand.Seed(time.Now().UnixNano())
}

var letterRunes = []rune("abcdefghijklmnopqrstuvwxyz")

func RandStringRunes(n int) string {
    b := make([]rune, n)
    for i := range b {
        b[i] = letterRunes[rand.Intn(len(letterRunes))]
    }
    return string(b)
}
