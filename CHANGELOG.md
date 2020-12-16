# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](http://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.8] - 2020-12-16

### Added
- Support custom cluster domain name via variable `ClusterDomainName` in cartrige chart `values.yaml`
- New chart for deploying ready-to-use crud based application
- Ability to change TARANTOOL_WORKDIR in the Cartridge helm chart and the **default value is set to** `/var/lib/tarantool`
