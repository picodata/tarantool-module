# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](http://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Makefile with a full set of targets for all occasions

### Changed
- The Tarantool Operator is installed in a separate namespace
- Bump operator-sdk version (and other dependencies)
- Refactor project structure, all helm charts are collected in one place
- Update crds api version from `apiextensions.k8s.io/v1beta1` to `apiextensions.k8s.io/v1`
- Updated kv example to the most recent cartridge version
- Update cartridge version for tarantool-cartridge and crud examples to the latest v2.7.3

### Fixed

- Operator was not able to manage multiple cartridge clusters in multiple namespaces

## [0.0.9] - 2021-03-30

### Added
- Integration test for cluster_controller written with envtest and ginkgo
- Description of failover setting in the Cartridge Kubernetes guide
- Section to troubleshooting about CrashLoopBackOff
- Lua memory reserve for tarantool containers
- Guide to troubleshooting about replicas recreating

### Changed
- Requested verbs for a RBAC role Tarantool: remove all * verbs and resources

### Fixed
- Not working update of replicaset roles
- Not working update of container env vars
- Problem with a non-existent leader of cluster
- Flaky role_controller unit test

## [0.0.8] - 2020-12-16

### Added
- Support custom cluster domain name via variable `ClusterDomainName` in cartrige chart `values.yaml`
- New chart for deploying ready-to-use crud based application
- Ability to change TARANTOOL_WORKDIR in the Cartridge helm chart and the **default value is set to** `/var/lib/tarantool`