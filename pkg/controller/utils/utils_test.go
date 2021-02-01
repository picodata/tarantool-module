package utils

import (
	. "github.com/onsi/ginkgo"
	. "github.com/onsi/gomega"
)

var _ = Describe("controller utils unit testing", func() {
	Describe("function IsRolesEquals must compare two arrays of strings for equality regardless of the order of elements", func() {
		Context("positive cases (equal arrays)", func() {
			It("should return True if both arrays are empty", func() {
				Expect(
					IsRolesEquals([]string{}, []string{}),
				).Should(BeTrue())
			})

			It("should return True if both arrays have len == 1", func() {
				Expect(
					IsRolesEquals([]string{"A"}, []string{"A"}),
				).Should(BeTrue())
			})

			It("should return True if the elements are arranged in the same order", func() {
				Expect(
					IsRolesEquals(
						[]string{"A", "B", "C", "D", "E"},
						[]string{"A", "B", "C", "D", "E"},
					),
				).Should(BeTrue())
			})

			It("should return True if the elements are arranged in a different order", func() {
				Expect(
					IsRolesEquals(
						[]string{"A", "B", "C", "D", "E"},
						[]string{"B", "D", "E", "A", "C"},
					),
				).Should(BeTrue())
			})
		})

		Context("negative cases (unequal arrays)", func() {
			It("should return False if one of the arrays is empty", func() {
				By("first empty")
				Expect(
					IsRolesEquals([]string{}, []string{"A"}),
				).Should(BeFalse())

				By("second empty")
				Expect(
					IsRolesEquals([]string{"A"}, []string{}),
				).Should(BeFalse())
			})

			It("should return False if arrays are the same size, but the elements are different", func() {
				Expect(
					IsRolesEquals(
						[]string{"A", "B", "C", "D", "E"},
						[]string{"Z", "F", "G", "C", "W"},
					),
				).Should(BeFalse())
			})
			It("should return False if arrays are the different size", func() {
				By("first more")
				Expect(
					IsRolesEquals(
						[]string{"A", "B", "C", "D", "E"},
						[]string{"A", "B", "C", "D"},
					),
				).Should(BeFalse())

				By("second more")
				Expect(
					IsRolesEquals(
						[]string{"A", "B", "C", "D"},
						[]string{"A", "B", "C", "D", "E"},
					),
				).Should(BeFalse())
			})
		})
	})
})
