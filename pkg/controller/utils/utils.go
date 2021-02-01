package utils

func IsRolesEquals(rolesA, rolesB []string) bool {
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
	return isSubset(rolesA, rolesB) && isSubset(rolesB, rolesA)
}
