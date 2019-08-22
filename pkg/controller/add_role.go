package controller

import (
	"github.com/tarantool/tarantool-operator/pkg/controller/role"
)

func init() {
	// AddToManagerFuncs is a list of functions to create controllers and add them to a manager.
	AddToManagerFuncs = append(AddToManagerFuncs, role.Add)
}
