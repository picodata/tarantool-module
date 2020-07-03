package e2e

import (
	"testing"
	"time"

	framework "github.com/operator-framework/operator-sdk/pkg/test"
	"github.com/operator-framework/operator-sdk/pkg/test/e2eutil"
	apierrors "k8s.io/apimachinery/pkg/api/errors"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/util/wait"
)

func TestOperatorMustCreateStatefulSetPerCartridgeRole(t *testing.T) {
	ctx := framework.NewTestCtx(t)
	defer ctx.Cleanup()

	clenupOpts := &framework.CleanupOptions{
		TestContext:   ctx,
		Timeout:       time.Second * 60,
		RetryInterval: time.Second * 1,
	}
	if err := ctx.InitializeClusterResources(clenupOpts); err != nil {
		t.Fatalf("failed to initialize cluster resources: %v", err)
	}
	t.Log("Initialized cluster resources")

	namespace, err := ctx.GetNamespace()
	if err != nil {
		t.Fatalf("failed to get namespace %s", err)
	}

	kubeClient := framework.Global.KubeClient
	err = e2eutil.WaitForOperatorDeployment(t, kubeClient, namespace, "tarantool-operator", 1, time.Second*1, time.Second*60)
	if err != nil {
		t.Fatalf("failed to deploy operator %s", err)
	}

	if err = InitializeScenario(ctx, "basic"); err != nil {
		t.Fatalf("failed to initialize scenario %s", err)
	}

	expectedRoles := 2
	err = wait.Poll(time.Second*1, time.Second*60, func() (done bool, err error) {
		sts, err := kubeClient.AppsV1().StatefulSets(namespace).List(metav1.ListOptions{})
		if err != nil {
			if apierrors.IsNotFound(err) {
				return false, nil
			}
			return false, err
		}

		if len(sts.Items) == expectedRoles {
			return true, nil
		}

		return false, nil
	})
	if err != nil {
		t.Fatal(err)
	}
}
