package e2e

import (
	"bufio"
	goctx "context"
	"fmt"
	"io"
	"os"
	"time"

	"github.com/ghodss/yaml"
	framework "github.com/operator-framework/operator-sdk/pkg/test"
	"k8s.io/apimachinery/pkg/apis/meta/v1/unstructured"
	k8syaml "k8s.io/apimachinery/pkg/util/yaml"
)

// InitializeScenario provisions initial system under test state
// from yaml kubernetes manifests
func InitializeScenario(ctx *framework.TestCtx, name string) error {
	yamlFile, err := os.Open(fmt.Sprintf("test/e2e/scenario/%s.yaml", name))
	if err != nil {
		return err
	}

	namespace, err := ctx.GetNamespace()
	if err != nil {
		return err
	}

	dec := k8syaml.NewYAMLReader(bufio.NewReader(yamlFile))
	res := []unstructured.Unstructured{}
	for {
		b, err := dec.Read()
		if err != nil {
			if err == io.EOF {
				break
			}
		}
		spec, err := yaml.YAMLToJSON(b)
		if err != nil {
			return err
		}

		obj := unstructured.Unstructured{}
		err = obj.UnmarshalJSON(spec)
		if err != nil {
			return err
		}
		obj.SetNamespace(namespace)
		res = append(res, obj)
	}
	for _, obj := range res {
		err = framework.Global.Client.Create(goctx.TODO(), &obj, &framework.CleanupOptions{TestContext: ctx, Timeout: time.Second * 5, RetryInterval: time.Second * 1})
		if err != nil {
			return err
		}
	}
	return nil
}
